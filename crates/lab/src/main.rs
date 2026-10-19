//! `harness-lab`: compare translation models (and inputs) on real catalogs.
//!
//! A maintainer tool, separate from the translator app. `bench` works on a
//! copy of the catalogs, translates a deterministic stratified sample with
//! every configuration through the same session code the app uses, and
//! writes the evidence: outputs, gate problems, timings, a blind judging
//! sheet (with its key in a separate file) and an HTML report. `score`
//! joins a filled judging sheet back to the configurations.

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use i18n_harness_core::UnitId;
use i18n_harness_session::{
    InputText, IssueSeverity, ModelSettings, Promotion, ServerKind, Session, UnitFilter,
};
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[command(
    name = "harness-lab",
    about = "Compare translation models on real catalogs"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Translate a sample of strings (drawn evenly across kinds of string)
    /// with each configuration and write the evidence to `--out`.
    Bench(BenchArgs),
    /// Score a filled judging sheet: reads `<out>/scores.jsonl` and
    /// `<out>/judge_key.json`, writes `<out>/scores-summary.json`.
    Score {
        /// The bench output folder.
        out: PathBuf,
    },
}

#[derive(Parser)]
struct BenchArgs {
    /// Folder with the catalogs (copied; never modified).
    folder: PathBuf,
    /// Configurations to compare, `model@auto` (pivot text when sound) or
    /// `model@source` (always the original). Repeat the flag.
    #[arg(long = "config", required = true)]
    configs: Vec<String>,
    /// Language the catalogs' existing texts are in, when it is not the
    /// catalogs' own language.
    #[arg(long)]
    pivot: Option<String>,
    /// Ollama address.
    #[arg(long, default_value = "http://localhost:11434")]
    host: String,
    /// Strings in the sample.
    #[arg(long, default_value_t = 200)]
    sample: usize,
    /// Seed for the sample and the blind letters.
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// Output folder.
    #[arg(long)]
    out: PathBuf,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Bench(args) => bench(&args),
        Command::Score { out } => score(&out),
    }
}

// ── sampling ────────────────────────────────────────────────────────────────

/// Deterministic xorshift64* generator (no dependency, stable across
/// platforms and releases).
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = usize::try_from(self.next() % (i as u64 + 1)).unwrap_or(0);
            items.swap(i, j);
        }
    }
}

/// What kind of string a sample item is; each item is counted in the first
/// stratum it matches.
const STRATA: &[(&str, f64)] = &[
    ("plural", 0.0),
    ("markup", 0.15),
    ("placeholder", 0.2),
    ("identifier", 0.1),
    ("no-pivot", 0.15),
    ("long", 0.1),
    ("short", 0.15),
    ("other", 0.15),
];

fn looks_like_identifier(word: &str) -> bool {
    let w = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
    let letters = w.chars().filter(char::is_ascii_alphabetic).count();
    let inner_upper = w.chars().skip(1).any(|c| c.is_ascii_uppercase())
        && w.chars().any(|c| c.is_ascii_lowercase());
    (w.contains('_') && letters > 1)
        || (letters >= 2
            && w.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
        || (inner_upper && letters >= 4)
}

fn stratum(input: &str, plural: bool, pivot_used: bool, pivot_expected: bool) -> &'static str {
    let placeholder = input.contains('%') || input.contains('{');
    if plural {
        "plural"
    } else if input.contains('<') && input.contains('>') {
        "markup"
    } else if placeholder {
        "placeholder"
    } else if input.split_whitespace().any(looks_like_identifier) {
        "identifier"
    } else if pivot_expected && !pivot_used {
        "no-pivot"
    } else if input.chars().count() > 60 {
        "long"
    } else if input.chars().count() <= 15 {
        "short"
    } else {
        "other"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Item {
    row: usize,
    catalog: String,
    id: String,
    stratum: String,
    /// The original text (native syntax).
    source: String,
    /// The pivot-language text, when the catalog had one.
    pivot: Option<String>,
}

fn copy_catalogs(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let name = entry.file_name();
            let lower = name.to_string_lossy().to_lowercase();
            if [".ts", ".po", ".pot", ".json"]
                .iter()
                .any(|e| lower.ends_with(e))
            {
                std::fs::copy(&path, to.join(&name))
                    .with_context(|| format!("copy {}", path.display()))?;
            }
        }
    }
    Ok(())
}

fn open_work(work: &Path, pivot: Option<&str>) -> Result<Session> {
    let mut session = Session::open_folder(work)?;
    if let Some(pivot) = pivot {
        let paths: Vec<PathBuf> = session
            .catalogs()
            .into_iter()
            .map(|c| c.path.into())
            .collect();
        for path in paths {
            session.set_pivot(&path, Some(pivot))?;
        }
    }
    Ok(session)
}

fn sample(session: &Session, size: usize, seed: u64, pivot_expected: bool) -> Result<Vec<Item>> {
    let mut by_stratum: BTreeMap<&str, Vec<Item>> = BTreeMap::new();
    let mut seen_inputs: HashSet<String> = HashSet::new();
    for catalog in session.catalogs() {
        let path = PathBuf::from(&catalog.path);
        let page = session.units(&path, &UnitFilter::default(), 0, usize::MAX)?;
        for unit in page.units.iter().filter(|u| u.editable) {
            let task = session.prepare(&path, &UnitId::from(unit.id.as_str()), InputText::Auto)?;
            let input = task
                .segments
                .first()
                .map(|s| s.request.source.clone())
                .unwrap_or_default();
            if input.trim().is_empty() || !seen_inputs.insert(input.clone()) {
                continue;
            }
            let pivot_used = task.input_locale != "en";
            let s = stratum(&unit.source, task.plural, pivot_used, pivot_expected);
            by_stratum.entry(s).or_default().push(Item {
                row: 0,
                catalog: catalog.path.clone(),
                id: unit.id.clone(),
                stratum: s.to_owned(),
                source: unit.source.clone(),
                pivot: unit
                    .existing
                    .as_ref()
                    .and_then(|e| e.first().cloned())
                    .filter(|t| !t.is_empty()),
            });
        }
    }
    let mut rng = Rng::new(seed);
    let mut picked = Vec::new();
    for (name, weight) in STRATA {
        let Some(items) = by_stratum.get_mut(name) else {
            continue;
        };
        rng.shuffle(items);
        // Every plural string is sampled (there are few); the rest by weight.
        let quota = if *weight == 0.0 {
            items.len()
        } else {
            ((size as f64) * weight).ceil() as usize
        };
        picked.extend(items.iter().take(quota).cloned());
    }
    picked.truncate(size.max(by_stratum.get("plural").map_or(0, Vec::len)));
    for (row, item) in picked.iter_mut().enumerate() {
        item.row = row + 1;
    }
    Ok(picked)
}

// ── running ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Output {
    config: String,
    row: usize,
    input_locale: String,
    output: Vec<String>,
    errors: Vec<String>,
    warnings: Vec<String>,
    failure: Option<String>,
    seconds: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct ConfigSummary {
    config: String,
    model: String,
    input: String,
    strings: usize,
    translated: usize,
    failed: usize,
    with_errors: usize,
    with_warnings: usize,
    median_seconds: f64,
    total_seconds: f64,
}

fn parse_config(spec: &str) -> Result<(String, InputText)> {
    let (model, input) = spec.rsplit_once('@').unwrap_or((spec, "auto"));
    let input = match input {
        "auto" => InputText::Auto,
        "source" => InputText::Source,
        other => bail!("unknown input `{other}` in `{spec}` (use @auto or @source)"),
    };
    Ok((model.to_owned(), input))
}

fn short_model(model: &str) -> String {
    model
        .rsplit('/')
        .next()
        .unwrap_or(model)
        .split(':')
        .next()
        .unwrap_or(model)
        .to_owned()
}

fn bench(args: &BenchArgs) -> Result<()> {
    std::fs::create_dir_all(&args.out)?;
    let work = args.out.join("work");
    if work.exists() {
        std::fs::remove_dir_all(&work)?;
    }
    copy_catalogs(&args.folder, &work)?;

    let session = open_work(&work, args.pivot.as_deref())?;
    let items = sample(&session, args.sample, args.seed, args.pivot.is_some())?;
    drop(session);
    eprintln!("sample: {} strings", items.len());
    write_jsonl(&args.out.join("sample.jsonl"), &items)?;

    let mut outputs: Vec<Output> = Vec::new();
    let mut summaries = Vec::new();
    for spec in &args.configs {
        match run_config(args, spec, &work, &items) {
            Ok((mine, summary)) => {
                summaries.push(summary);
                outputs.extend(mine);
            }
            Err(error) => eprintln!("{spec}: skipped: {error:#}"),
        }
        // Keep what is finished even if a later configuration fails.
        write_jsonl(&args.out.join("outputs.jsonl"), &outputs)?;
        std::fs::write(
            args.out.join("summary.json"),
            serde_json::to_string_pretty(&summaries)?,
        )?;
    }
    write_judging(&args.out, &items, &outputs, &summaries, args.seed)?;
    std::fs::write(
        args.out.join("report.html"),
        report_html(&items, &outputs, &summaries),
    )?;
    std::fs::remove_dir_all(&work).ok();
    eprintln!("wrote {}", args.out.display());
    Ok(())
}

/// Translate the sample with one configuration on a fresh session.
fn run_config(
    args: &BenchArgs,
    spec: &str,
    work: &Path,
    items: &[Item],
) -> Result<(Vec<Output>, ConfigSummary)> {
    let (model, input) = parse_config(spec)?;
    let config = format!(
        "{}@{}",
        short_model(&model),
        if input == InputText::Auto {
            "auto"
        } else {
            "source"
        }
    );
    let mut session = open_work(work, args.pivot.as_deref())?;
    let settings = ModelSettings {
        server: ServerKind::Ollama,
        host: args.host.clone(),
        model: Some(model.clone()),
        num_ctx: None,
    };
    let backend = i18n_harness_session::backend(&settings)?;
    // Load the model before timing anything.
    if let Some(first) = items.first() {
        let task = session.prepare(
            Path::new(&first.catalog),
            &UnitId::from(first.id.as_str()),
            input,
        )?;
        i18n_harness_session::translate_task(backend.as_ref(), &task)?;
    }
    let mut mine = Vec::new();
    for item in items {
        let path = PathBuf::from(&item.catalog);
        let task = session.prepare(&path, &UnitId::from(item.id.as_str()), input)?;
        let started = Instant::now();
        let outcome = i18n_harness_session::translate_task(backend.as_ref(), &task)?;
        let seconds = started.elapsed().as_secs_f64();
        let applied = session.apply(&task, outcome, Promotion::Review)?;
        let pick = |sev: IssueSeverity| {
            applied
                .unit
                .issues
                .iter()
                .filter(|i| i.severity == sev)
                .map(|i| i.message.clone())
                .collect::<Vec<_>>()
        };
        mine.push(Output {
            config: config.clone(),
            row: item.row,
            input_locale: task.input_locale.clone(),
            output: if applied.translated {
                applied.unit.target.clone()
            } else {
                Vec::new()
            },
            errors: pick(IssueSeverity::Error),
            warnings: pick(IssueSeverity::Warning),
            failure: applied.failure,
            seconds,
        });
    }
    let mut times: Vec<f64> = mine.iter().map(|o| o.seconds).collect();
    times.sort_by(f64::total_cmp);
    let summary = ConfigSummary {
        config: config.clone(),
        model: model.clone(),
        input: if input == InputText::Auto {
            "auto".into()
        } else {
            "source".into()
        },
        strings: mine.len(),
        translated: mine.iter().filter(|o| o.failure.is_none()).count(),
        failed: mine.iter().filter(|o| o.failure.is_some()).count(),
        with_errors: mine.iter().filter(|o| !o.errors.is_empty()).count(),
        with_warnings: mine.iter().filter(|o| !o.warnings.is_empty()).count(),
        median_seconds: times.get(times.len() / 2).copied().unwrap_or(0.0),
        total_seconds: times.iter().sum(),
    };
    eprintln!(
        "{config}: {}/{} translated, {} with errors, {} with warnings, median {:.2}s",
        summary.translated,
        summary.strings,
        summary.with_errors,
        summary.with_warnings,
        summary.median_seconds
    );
    Ok((mine, summary))
}

fn write_jsonl<T: Serialize>(path: &Path, rows: &[T]) -> Result<()> {
    let mut text = String::new();
    for row in rows {
        text.push_str(&serde_json::to_string(row)?);
        text.push('\n');
    }
    std::fs::write(path, text).with_context(|| format!("write {}", path.display()))
}

// ── blind judging ───────────────────────────────────────────────────────────

/// Writes `judge_sheet.md` (each row's outputs under shuffled letters, no
/// configuration names) and `judge_key.json` (letter → configuration per
/// row). Identical outputs share a letter so a judge scores them once.
fn write_judging(
    out: &Path,
    items: &[Item],
    outputs: &[Output],
    summaries: &[ConfigSummary],
    seed: u64,
) -> Result<()> {
    let mut rng = Rng::new(seed ^ 0x5EED);
    let mut sheet = String::from(
        "# Judging sheet\n\nScore each candidate 1–5 for how well it says what the original says, in natural, \
         correct target-language UI style, keeping placeholders, markup and identifiers. Configuration names are \
         hidden; see judge_key.json only after scoring.\n\n",
    );
    let mut key: BTreeMap<usize, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for item in items {
        let mut candidates: Vec<(String, Vec<String>)> = Vec::new();
        for s in summaries {
            let text = outputs
                .iter()
                .find(|o| o.row == item.row && o.config == s.config)
                .map(|o| {
                    if o.output.is_empty() {
                        "(no translation)".to_owned()
                    } else {
                        o.output.join(" | ")
                    }
                })
                .unwrap_or_default();
            match candidates.iter_mut().find(|(t, _)| *t == text) {
                Some((_, configs)) => configs.push(s.config.clone()),
                None => candidates.push((text, vec![s.config.clone()])),
            }
        }
        rng.shuffle(&mut candidates);
        let _ = writeln!(sheet, "## {} ({})\n", item.row, item.stratum);
        let _ = writeln!(sheet, "- Original: `{}`", item.source);
        if let Some(pivot) = &item.pivot {
            let _ = writeln!(sheet, "- Existing: `{pivot}`");
        }
        let letters = key.entry(item.row).or_default();
        for (i, (text, configs)) in candidates.into_iter().enumerate() {
            let letter = char::from(b'A' + u8::try_from(i).unwrap_or(25)).to_string();
            let _ = writeln!(sheet, "- {letter}: `{text}`");
            letters.insert(letter, configs);
        }
        sheet.push('\n');
    }
    std::fs::write(out.join("judge_sheet.md"), sheet)?;
    std::fs::write(
        out.join("judge_key.json"),
        serde_json::to_string_pretty(&key)?,
    )?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ScoreLine {
    row: usize,
    scores: BTreeMap<String, f64>,
}

fn score(out: &Path) -> Result<()> {
    let key: BTreeMap<usize, BTreeMap<String, Vec<String>>> =
        serde_json::from_str(&std::fs::read_to_string(out.join("judge_key.json"))?)?;
    let items: Vec<Item> = std::fs::read_to_string(out.join("sample.jsonl"))?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let strata: BTreeMap<usize, String> = items.into_iter().map(|i| (i.row, i.stratum)).collect();
    let mut totals: BTreeMap<String, (f64, usize)> = BTreeMap::new();
    let mut by_stratum: BTreeMap<(String, String), (f64, usize)> = BTreeMap::new();
    let mut wins: BTreeMap<String, f64> = BTreeMap::new();
    for line in std::fs::read_to_string(out.join("scores.jsonl"))?
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let line: ScoreLine = serde_json::from_str(line)?;
        let letters = key
            .get(&line.row)
            .with_context(|| format!("row {} not in key", line.row))?;
        let best = line.scores.values().copied().fold(f64::MIN, f64::max);
        let winners: Vec<&String> = line
            .scores
            .iter()
            .filter(|(_, s)| **s == best)
            .map(|(l, _)| l)
            .collect();
        for (letter, value) in &line.scores {
            for config in letters.get(letter).into_iter().flatten() {
                let t = totals.entry(config.clone()).or_default();
                t.0 += value;
                t.1 += 1;
                let s = by_stratum
                    .entry((
                        config.clone(),
                        strata.get(&line.row).cloned().unwrap_or_default(),
                    ))
                    .or_default();
                s.0 += value;
                s.1 += 1;
                if winners.contains(&letter) {
                    *wins.entry(config.clone()).or_default() += 1.0 / winners.len() as f64;
                }
            }
        }
    }
    let summary = serde_json::json!({
        "mean_score": totals.iter().map(|(c, (s, n))| (c.clone(), s / *n as f64)).collect::<BTreeMap<_, _>>(),
        "rows_scored": totals.values().map(|(_, n)| *n).max().unwrap_or(0),
        "best_share": wins,
        "mean_score_by_stratum": by_stratum
            .iter()
            .map(|((c, st), (s, n))| (format!("{c} / {st}"), s / *n as f64))
            .collect::<BTreeMap<_, _>>(),
    });
    std::fs::write(
        out.join("scores-summary.json"),
        serde_json::to_string_pretty(&summary)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

// ── report ──────────────────────────────────────────────────────────────────

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn report_html(items: &[Item], outputs: &[Output], summaries: &[ConfigSummary]) -> String {
    let mut h = String::from(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" \
         content=\"width=device-width,initial-scale=1\"><title>Model benchmark</title><style>\
         :root{--bg:#fafafa;--fg:#1d1d1f;--muted:#6b6b70;--line:#dedee3;--err:#b3261e;--warn:#8a5a00}\
         @media(prefers-color-scheme:dark){:root{--bg:#161618;--fg:#ececf0;--muted:#a0a0a8;--line:#34343a;--err:#ff8a80;--warn:#f0c060}}\
         body{font:14px/1.45 system-ui,sans-serif;background:var(--bg);color:var(--fg);margin:0;padding:16px}\
         table{border-collapse:collapse;width:100%}td,th{border-bottom:1px solid var(--line);padding:6px 8px;\
         text-align:left;vertical-align:top}th{color:var(--muted);font-weight:500}.m{color:var(--muted)}\
         .e{color:var(--err)}.w{color:var(--warn)}code{white-space:pre-wrap}.wrap{overflow-x:auto}\
         </style></head><body><h1>Model benchmark</h1><div class=\"wrap\"><table><tr><th>Configuration</th>\
         <th>Translated</th><th>With errors</th><th>With warnings</th><th>Median s</th><th>Total s</th></tr>",
    );
    for s in summaries {
        let _ = write!(
            h,
            "<tr><td>{}</td><td>{}/{}</td><td>{}</td><td>{}</td><td>{:.2}</td><td>{:.0}</td></tr>",
            esc(&s.config),
            s.translated,
            s.strings,
            s.with_errors,
            s.with_warnings,
            s.median_seconds,
            s.total_seconds
        );
    }
    h.push_str("</table></div><h2>Samples</h2><div class=\"wrap\"><table><tr><th>#</th><th>Original / existing</th>");
    for s in summaries {
        let _ = write!(h, "<th>{}</th>", esc(&s.config));
    }
    h.push_str("</tr>");
    for item in items {
        let _ = write!(
            h,
            "<tr><td>{}<br><span class=\"m\">{}</span></td><td><code>{}</code>{}</td>",
            item.row,
            esc(&item.stratum),
            esc(&item.source),
            item.pivot
                .as_deref()
                .map(|p| format!("<br><code class=\"m\">{}</code>", esc(p)))
                .unwrap_or_default()
        );
        for s in summaries {
            let cell = outputs
                .iter()
                .find(|o| o.row == item.row && o.config == s.config);
            let _ = write!(h, "<td>");
            if let Some(o) = cell {
                let _ = write!(h, "<code>{}</code>", esc(&o.output.join(" | ")));
                for e in &o.errors {
                    let _ = write!(h, "<br><span class=\"e\">{}</span>", esc(e));
                }
                for w in &o.warnings {
                    let _ = write!(h, "<br><span class=\"w\">{}</span>", esc(w));
                }
                if let Some(f) = &o.failure {
                    let _ = write!(h, "<br><span class=\"e\">failed: {}</span>", esc(f));
                }
            }
            let _ = write!(h, "</td>");
        }
        h.push_str("</tr>");
    }
    h.push_str("</table></div></body></html>\n");
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_recognized() {
        for w in ["ParamsManager", "CDepthBitmap", "PF_XYZ32", "ADS", "AOI"] {
            assert!(looks_like_identifier(w), "{w}");
        }
        for w in ["Filter", "mm", "Size", "a"] {
            assert!(!looks_like_identifier(w), "{w}");
        }
    }

    #[test]
    fn the_shuffle_is_deterministic() {
        let mut a: Vec<u32> = (0..20).collect();
        let mut b = a.clone();
        Rng::new(7).shuffle(&mut a);
        Rng::new(7).shuffle(&mut b);
        assert_eq!(a, b);
        assert_ne!(a, (0..20).collect::<Vec<_>>());
    }

    #[test]
    fn configs_parse() {
        assert_eq!(parse_config("m:q8@source").unwrap().1, InputText::Source);
        assert_eq!(
            parse_config("hf.co/x/Y-GGUF:Q8_0").unwrap().0,
            "hf.co/x/Y-GGUF:Q8_0"
        );
        assert!(parse_config("m@weird").is_err());
        assert_eq!(
            short_model("hf.co/mradermacher/EuroLLM-9B-Instruct-2512-GGUF:Q8_0"),
            "EuroLLM-9B-Instruct-2512-GGUF"
        );
    }
}
