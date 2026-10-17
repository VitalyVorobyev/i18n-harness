//! Evaluation store for in-app prompt evaluation runs.
//!
//! [`EvaluationRun`] records the outcome of running the current prompt over
//! every curated example. [`EvaluationStore`] is an append-only JSONL store
//! that mirrors the shape of [`crate::memory::CorrectionStore`].
//!
//! Scoring is v1 exact-match: score = 1.0 when `llm_output.trim() ==
//! human_target.trim()`, 0.0 otherwise. Smarter metrics (BLEU, chrF) are a
//! future iteration.
//!
//! See `docs/roadmap-product.md` for the full specification.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use i18n_harness_core::Flag;

use crate::error::ProjectError;
use crate::fs::ProjectFs;

// ── Score types ───────────────────────────────────────────────────────────────

/// Per-locale aggregated score for one evaluation run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocaleScore {
    /// Mean exact-match score across all curated examples for this locale.
    /// `NaN` is never produced — the `f32` is always in `[0.0, 1.0]`.
    pub score: f32,
    /// Number of examples scored in this locale.
    pub count: usize,
}

/// Per-flag-kind aggregated score for one evaluation run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlagScore {
    /// Mean exact-match score across examples where `flags_at_correction`
    /// contained this flag kind.
    pub score: f32,
    /// Number of examples with this flag kind.
    pub count: usize,
}

// ── EvaluationRun ─────────────────────────────────────────────────────────────

/// Result of one in-app prompt evaluation run, stored as a single JSON line
/// in `evaluations.jsonl`.
///
/// Each run re-translates every curated example through the configured backend
/// and compares the output (after `.trim()`) to the `human_target`. The
/// per-locale and per-flag-kind breakdowns allow the user to see where prompt
/// changes helped or hurt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationRun {
    /// Schema version. Currently `1`.
    pub schema: u32,
    /// RFC 3339 UTC timestamp of when the run completed.
    pub ts: String,
    /// Prompt template version identifier, e.g. `"ollama-translate-v2"`.
    pub prompt_template_version: String,
    /// Overall mean exact-match score across all examples. In `[0.0, 1.0]`.
    pub overall_score: f32,
    /// Scores broken down by target locale id.
    pub per_locale: BTreeMap<String, LocaleScore>,
    /// Scores broken down by the flag kinds present on each example at the
    /// time it was promoted to the curated set. Keys are serde kebab-case
    /// [`Flag`] names. Only flags with at least one example are included.
    pub per_flag_kind: BTreeMap<String, FlagScore>,
    /// Total number of curated examples evaluated.
    pub example_count: usize,
}

// ── Score aggregation ─────────────────────────────────────────────────────────

/// Accumulated score buckets used while building an [`EvaluationRun`] in the
/// evaluation worker. Call [`ScoreAccumulator::finish`] to produce the final
/// `EvaluationRun`.
#[derive(Debug, Default)]
pub struct ScoreAccumulator {
    /// `(sum_of_scores, count)` per locale.
    locale_buckets: BTreeMap<String, (f64, usize)>,
    /// `(sum_of_scores, count)` per flag kind (kebab-case string).
    flag_buckets: BTreeMap<String, (f64, usize)>,
    /// Total examples and total score sum, for the overall mean.
    total_sum: f64,
    total_count: usize,
}

impl ScoreAccumulator {
    /// Create an empty accumulator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the score for one example.
    ///
    /// `locale` is the target locale id. `flags` is the set of flag kind names
    /// (kebab-case) the example had at curation time. `score` must be `0.0` or
    /// `1.0` for v1 exact-match scoring.
    pub fn record(&mut self, locale: &str, flags: &[String], score: f32) {
        let s = f64::from(score);

        let (sum, cnt) = self.locale_buckets.entry(locale.to_owned()).or_default();
        *sum += s;
        *cnt += 1;

        for flag in flags {
            let (fsum, fcnt) = self.flag_buckets.entry(flag.clone()).or_default();
            *fsum += s;
            *fcnt += 1;
        }

        self.total_sum += s;
        self.total_count += 1;
    }

    /// Consume the accumulator and return a completed [`EvaluationRun`].
    pub fn finish(self, ts: String, prompt_template_version: String) -> EvaluationRun {
        let overall_score = if self.total_count == 0 {
            0.0
        } else {
            (self.total_sum / self.total_count as f64) as f32
        };

        let per_locale = self
            .locale_buckets
            .into_iter()
            .map(|(locale, (sum, count))| {
                let score = if count == 0 {
                    0.0
                } else {
                    (sum / count as f64) as f32
                };
                (locale, LocaleScore { score, count })
            })
            .collect();

        let per_flag_kind = self
            .flag_buckets
            .into_iter()
            .map(|(flag, (sum, count))| {
                let score = if count == 0 {
                    0.0
                } else {
                    (sum / count as f64) as f32
                };
                (flag, FlagScore { score, count })
            })
            .collect();

        EvaluationRun {
            schema: 1,
            ts,
            prompt_template_version,
            overall_score,
            per_locale,
            per_flag_kind,
            example_count: self.total_count,
        }
    }
}

// ── EvaluationStore ───────────────────────────────────────────────────────────

/// Append-only evaluation-run store backed by `<state_dir>/evaluations.jsonl`.
///
/// One line per completed [`EvaluationRun`]. Cheap to construct — I/O happens
/// only on [`append`](Self::append) and [`list`](Self::list).
///
/// `Clone` is cheap (only an `Arc` bump) and is provided so the Tauri layer
/// can capture a snapshot of a project's evaluation store at job-spawn time.
/// This binds long-running evaluation workers to the project that started
/// them, so a subsequent `open_project` / `close_project` cannot redirect
/// the write to a different project. See codex P1 on PR #36.
#[derive(Clone)]
pub struct EvaluationStore {
    path: PathBuf,
    fs: Arc<dyn ProjectFs>,
}

impl EvaluationStore {
    /// Construct a store pointing at `path`. `fs` provides the I/O backing.
    ///
    /// The file need not exist at construction time; it is created lazily on
    /// the first append.
    pub fn new(path: PathBuf, fs: Arc<dyn ProjectFs>) -> Self {
        Self { path, fs }
    }

    /// Append a single evaluation run to the JSONL file.
    ///
    /// # Errors
    ///
    /// - [`ProjectError::Io`] if the filesystem append fails.
    pub fn append(&self, run: &EvaluationRun) -> Result<(), ProjectError> {
        let line = serde_json::to_string(run).expect("EvaluationRun serialization is infallible");
        debug_assert!(
            !line.contains('\n'),
            "EvaluationRun JSON must not contain literal newlines"
        );
        let mut bytes = line.into_bytes();
        bytes.push(b'\n');
        self.fs
            .append(&self.path, &bytes)
            .map_err(|source| ProjectError::Io {
                path: self.path.clone(),
                source,
            })
    }

    /// Read all evaluation runs from the JSONL file, in file order (oldest
    /// first).
    ///
    /// Returns an empty `Vec` if the file does not exist yet. Malformed lines
    /// are silently skipped (line-recoverable, matching `CorrectionStore`
    /// precedent).
    ///
    /// # Errors
    ///
    /// - [`ProjectError::Io`] if the file exists but cannot be read.
    pub fn list(&self) -> Result<Vec<EvaluationRun>, ProjectError> {
        if !self.fs.exists(&self.path) {
            return Ok(Vec::new());
        }

        let text = self
            .fs
            .read_to_string(&self.path)
            .map_err(|source| ProjectError::Io {
                path: self.path.clone(),
                source,
            })?;

        let runs = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|line| serde_json::from_str::<EvaluationRun>(line).ok())
            .collect();

        Ok(runs)
    }

    /// Return the most recently appended run, or `None` if the file is absent
    /// or empty.
    ///
    /// # Errors
    ///
    /// - [`ProjectError::Io`] if the file exists but cannot be read.
    pub fn latest(&self) -> Result<Option<EvaluationRun>, ProjectError> {
        Ok(self.list()?.into_iter().next_back())
    }
}

// ── Flag serialization helpers ────────────────────────────────────────────────

/// Convert a slice of [`Flag`] values to their kebab-case serde string
/// representations, suitable for use as `per_flag_kind` keys in
/// [`EvaluationRun`].
pub fn flags_to_strings(flags: &[Flag]) -> Vec<String> {
    flags
        .iter()
        .map(|f| {
            // Use serde_json to get the canonical kebab-case rendering.
            serde_json::to_value(f)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_else(|| format!("{f:?}"))
        })
        .collect()
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── ScoreAccumulator arithmetic ───────────────────────────────────────────

    #[test]
    fn empty_accumulator_produces_zero_overall_score() {
        let acc = ScoreAccumulator::new();
        let run = acc.finish("2026-01-01T00:00:00Z".into(), "v1".into());
        assert_eq!(run.overall_score, 0.0);
        assert_eq!(run.example_count, 0);
        assert!(run.per_locale.is_empty());
        assert!(run.per_flag_kind.is_empty());
    }

    #[test]
    fn all_correct_produces_score_one() {
        let mut acc = ScoreAccumulator::new();
        acc.record("de_DE", &[], 1.0);
        acc.record("de_DE", &[], 1.0);
        let run = acc.finish("ts".into(), "v1".into());
        assert!((run.overall_score - 1.0).abs() < f32::EPSILON);
        let ls = &run.per_locale["de_DE"];
        assert!((ls.score - 1.0).abs() < f32::EPSILON);
        assert_eq!(ls.count, 2);
    }

    #[test]
    fn all_wrong_produces_score_zero() {
        let mut acc = ScoreAccumulator::new();
        acc.record("fr_FR", &[], 0.0);
        acc.record("fr_FR", &[], 0.0);
        let run = acc.finish("ts".into(), "v1".into());
        assert!((run.overall_score - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn mixed_scores_average_correctly() {
        let mut acc = ScoreAccumulator::new();
        acc.record("de_DE", &[], 1.0);
        acc.record("de_DE", &[], 0.0);
        acc.record("fr_FR", &[], 1.0);
        let run = acc.finish("ts".into(), "v1".into());
        // Overall: (1 + 0 + 1) / 3 = 2/3 ≈ 0.6667
        assert!((run.overall_score - 2.0 / 3.0).abs() < 1e-5);
        let de = &run.per_locale["de_DE"];
        assert!((de.score - 0.5).abs() < 1e-5);
        assert_eq!(de.count, 2);
        let fr = &run.per_locale["fr_FR"];
        assert!((fr.score - 1.0).abs() < f32::EPSILON);
        assert_eq!(fr.count, 1);
    }

    #[test]
    fn flag_buckets_aggregate_correctly() {
        let mut acc = ScoreAccumulator::new();
        // Two examples with "idiom" flag: one correct, one wrong.
        acc.record("de_DE", &["idiom".into()], 1.0);
        acc.record("de_DE", &["idiom".into()], 0.0);
        // One example with no flags.
        acc.record("de_DE", &[], 1.0);
        let run = acc.finish("ts".into(), "v1".into());
        let fs = &run.per_flag_kind["idiom"];
        assert!((fs.score - 0.5).abs() < 1e-5);
        assert_eq!(fs.count, 2);
    }

    #[test]
    fn single_locale_score_zero_count_guard() {
        // An accumulator with zero entries per locale should not divide by zero.
        // In practice the locale bucket is only created when `record` is called,
        // but the finish() branch `if count == 0 { 0.0 }` is exercised here by
        // manipulating the bucket directly.
        let acc = ScoreAccumulator {
            locale_buckets: {
                let mut m = BTreeMap::new();
                m.insert("ja_JP".into(), (0.0, 0));
                m
            },
            ..Default::default()
        };
        let run = acc.finish("ts".into(), "v1".into());
        let ls = &run.per_locale["ja_JP"];
        assert!((ls.score - 0.0).abs() < f32::EPSILON);
    }

    // ── EvaluationStore I/O ───────────────────────────────────────────────────

    #[test]
    fn store_append_list_latest_roundtrip() {
        use crate::fs::{InMemoryFs, ProjectFs};
        use std::sync::Arc;

        let inner = Arc::new(InMemoryFs::default());
        let fs: Arc<dyn ProjectFs> = Arc::clone(&inner) as Arc<dyn ProjectFs>;
        let path = std::path::PathBuf::from("/state/evaluations.jsonl");
        let store = EvaluationStore::new(path, fs);

        // Empty file — list and latest return nothing.
        assert!(store.list().unwrap().is_empty());
        assert!(store.latest().unwrap().is_none());

        // Append first run.
        let run1 = EvaluationRun {
            schema: 1,
            ts: "2026-01-01T00:00:00Z".into(),
            prompt_template_version: "v1".into(),
            overall_score: 0.75,
            per_locale: BTreeMap::new(),
            per_flag_kind: BTreeMap::new(),
            example_count: 4,
        };
        store.append(&run1).unwrap();

        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);
        assert!((list[0].overall_score - 0.75).abs() < f32::EPSILON);

        // Append second run.
        let run2 = EvaluationRun {
            schema: 1,
            ts: "2026-01-02T00:00:00Z".into(),
            prompt_template_version: "v2".into(),
            overall_score: 0.9,
            per_locale: BTreeMap::new(),
            per_flag_kind: BTreeMap::new(),
            example_count: 4,
        };
        store.append(&run2).unwrap();

        let list = store.list().unwrap();
        assert_eq!(list.len(), 2);

        // latest() returns the second run.
        let latest = store.latest().unwrap().unwrap();
        assert!((latest.overall_score - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn store_tolerates_missing_file() {
        use crate::fs::{InMemoryFs, ProjectFs};
        use std::sync::Arc;

        let inner = Arc::new(InMemoryFs::default());
        let fs: Arc<dyn ProjectFs> = Arc::clone(&inner) as Arc<dyn ProjectFs>;
        let store = EvaluationStore::new("/no/such/file.jsonl".into(), fs);
        assert!(store.list().unwrap().is_empty());
        assert!(store.latest().unwrap().is_none());
    }

    #[test]
    fn store_skips_malformed_lines() {
        use crate::fs::{InMemoryFs, ProjectFs};
        use std::sync::Arc;

        let inner = Arc::new(InMemoryFs::default());
        let path = std::path::PathBuf::from("/state/evaluations.jsonl");

        // Write a good line, a bad line, and another good line directly via
        // the InMemoryFs before constructing the store.
        let good = EvaluationRun {
            schema: 1,
            ts: "2026-01-01T00:00:00Z".into(),
            prompt_template_version: "v1".into(),
            overall_score: 0.5,
            per_locale: BTreeMap::new(),
            per_flag_kind: BTreeMap::new(),
            example_count: 2,
        };
        let good_json = serde_json::to_string(&good).unwrap();
        let contents = format!("{good_json}\n{{not valid json}}\n{good_json}\n");
        inner.write_atomic(&path, contents.as_bytes()).unwrap();

        let fs: Arc<dyn ProjectFs> = Arc::clone(&inner) as Arc<dyn ProjectFs>;
        let store = EvaluationStore::new(path.clone(), fs);

        let list = store.list().unwrap();
        assert_eq!(list.len(), 2, "malformed line should be skipped");
    }
}
