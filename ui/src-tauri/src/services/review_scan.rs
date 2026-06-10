//! Scan all open project catalogs and collect items that need review.
//!
//! `collect` is the single entry point; the `scan_project_review_state` Tauri
//! command delegates to it immediately after acquiring the `AppState` reference.
//! `truncate_preview` and `extract_target_text` are helpers that existed in
//! `lib.rs` before the split; both are tested here.

use std::collections::BTreeMap;
use std::path::PathBuf;

use i18n_harness_core::{ReviewStatus, UnitId, UnitState};

use crate::dto::project::{CatalogStateCounts, ReviewQueueItem, ReviewQueueResponse};
use crate::error;
use crate::state::AppState;
use crate::{OpenCatalogEntry, backing::extract_for_format};

/// Walk every catalog registered in the currently-open project, ensuring each
/// is cached in `project_catalogs`, then collect all units that need review
/// into a `ReviewQueueResponse`.
///
/// Catalogs already in the store are scanned from memory. Catalogs not yet
/// open are extracted from disk via the format-aware dispatcher and folded
/// with `Project::apply_review_state`, then inserted into the store with
/// `dirty: false` — the same side-effect as `open_catalog_in_project`.
///
/// Non-`qt-ts`/`gettext-po`/`icu-json` catalogs are skipped with a
/// `tracing::warn!` until their adapters are wired.
pub(crate) fn collect(state: &AppState) -> Result<ReviewQueueResponse, String> {
    // Collect the list of catalog refs from the project while holding the
    // project lock; drop the lock before any I/O so we do not hold it across
    // extract calls.
    // Snapshot the catalog list and the folded reviewer notes under one brief
    // project lock, then drop it before any I/O. The note map is keyed by the
    // manifest-relative catalog path + unit id, matching the review store's
    // fold; it carries the reference-conflict candidate JSON so the conflict
    // view survives a reopen.
    let (catalog_refs, review_notes): (
        Vec<i18n_harness_project::CatalogRef>,
        BTreeMap<(PathBuf, UnitId), String>,
    ) = {
        let project_guard = state
            .project
            .lock()
            .map_err(error::lock_poisoned("project"))?;
        let project = project_guard.as_ref().ok_or_else(error::no_project)?;
        let refs = project.catalogs().to_vec();
        let notes = project
            .review_map()
            .iter()
            .filter_map(|((catalog, unit_id), record)| {
                record
                    .reviewer_note
                    .clone()
                    .map(|note| ((catalog.clone(), unit_id.clone()), note))
            })
            .collect();
        (refs, notes)
    };

    // For each catalog, ensure it is in the project_catalogs store.
    // If it is already open, skip the I/O; otherwise extract + apply and insert.
    for catalog_ref in &catalog_refs {
        // gettext-po and icu-json are both wired; all manifest
        // formats are now handled through `extract_for_format`. The guard
        // remains in case future formats land in the manifest enum before
        // their wiring is finished — surface the gap loudly rather than
        // failing later in extract.
        if !matches!(
            catalog_ref.format,
            i18n_harness_project::CatalogFormat::QtTs
                | i18n_harness_project::CatalogFormat::GettextPo
                | i18n_harness_project::CatalogFormat::IcuJson,
        ) {
            tracing::warn!(
                path = %catalog_ref.manifest_path,
                format = ?catalog_ref.format,
                "scan_project_review_state: format not yet wired"
            );
            continue;
        }

        let abs = PathBuf::from(&catalog_ref.absolute_path);

        // Check whether already cached.
        let already_open = {
            let store = state
                .project_catalogs
                .lock()
                .map_err(error::lock_poisoned("project_catalogs"))?;
            store.contains_key(&abs)
        };

        if !already_open {
            // Extract from disk via the format-aware backing dispatcher.
            let mut catalog = extract_for_format(&abs, catalog_ref.format)
                .map_err(|e| format!("{} ({}): {e}", catalog_ref.manifest_path, "extract"))?;

            // Fold review state in.
            {
                let project_guard = state
                    .project
                    .lock()
                    .map_err(error::lock_poisoned("project"))?;
                if let Some(project) = project_guard.as_ref() {
                    project.apply_review_state(&abs, catalog.units_mut());
                }
            }

            // Insert into the store.
            state
                .project_catalogs
                .lock()
                .map_err(error::lock_poisoned("project_catalogs"))?
                .insert(
                    abs.clone(),
                    OpenCatalogEntry {
                        catalog,
                        dirty: false,
                    },
                );
        }
    }

    // Now scan all open catalogs and collect review items.
    let store = state
        .project_catalogs
        .lock()
        .map_err(error::lock_poisoned("project_catalogs"))?;

    // Build a manifest-path lookup by absolute path.
    let manifest_path_of: std::collections::HashMap<String, String> = catalog_refs
        .iter()
        .map(|r| (r.absolute_path.clone(), r.manifest_path.clone()))
        .collect();
    let locale_of: std::collections::HashMap<String, String> = catalog_refs
        .iter()
        .map(|r| (r.absolute_path.clone(), r.locale.clone()))
        .collect();

    let mut items: Vec<ReviewQueueItem> = Vec::new();
    let mut by_catalog: BTreeMap<String, usize> = BTreeMap::new();
    let mut stats_by_catalog: BTreeMap<String, CatalogStateCounts> = BTreeMap::new();

    // Iterate in BTreeMap order (= absolute path order) for deterministic output.
    for (abs, entry) in store.iter() {
        let abs_str = abs.to_string_lossy().into_owned();
        let manifest_path = manifest_path_of
            .get(&abs_str)
            .cloned()
            .unwrap_or_else(|| abs_str.clone());
        let locale = locale_of.get(&abs_str).cloned().unwrap_or_default();

        let mut catalog_count: usize = 0;
        let mut counts = CatalogStateCounts::default();

        for unit in entry.catalog.units() {
            counts.total += 1;
            match unit.state {
                UnitState::Finished => counts.finished += 1,
                UnitState::Proposed => counts.proposed += 1,
                UnitState::Untranslated => counts.untranslated += 1,
                UnitState::Vanished | UnitState::Obsolete => counts.vanished_obsolete += 1,
            }

            // Serialize flags via serde to get the kebab-case strings that the
            // `#[serde(rename_all = "kebab-case")]` attribute on `Flag` produces.
            // `format!("{:?}")` would give PascalCase debug output instead.
            let flags: Vec<String> = unit
                .flags
                .iter()
                .filter_map(|f| {
                    serde_json::to_value(f)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_owned))
                })
                .collect();

            let needs_review = matches!(
                unit.review_status,
                Some(ReviewStatus::NeedsReview | ReviewStatus::Conflict)
            ) || !flags.is_empty();
            if !needs_review {
                continue;
            }
            counts.needs_review += 1;

            let source_preview = truncate_preview(&unit.source, 120);
            let target_preview = extract_target_text(&unit.target, 120);

            // Serialize review_status via serde for the kebab-case string.
            let review_status_str: Option<String> = unit.review_status.and_then(|s| {
                serde_json::to_value(s)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
            });

            // Serialize state via serde for the kebab-case string.
            let state_str: String = serde_json::to_value(unit.state)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_else(|| "unknown".to_string());

            let reviewer_note = review_notes
                .get(&(PathBuf::from(&manifest_path), unit.id.clone()))
                .cloned();

            items.push(ReviewQueueItem {
                catalog_path: abs_str.clone(),
                catalog_manifest_path: manifest_path.clone(),
                locale: locale.clone(),
                unit_id: unit.id.to_string(),
                source_preview,
                target_preview,
                flags,
                review_status: review_status_str,
                state: state_str.to_string(),
                reviewer_note,
            });
            catalog_count += 1;
        }

        if catalog_count > 0 {
            by_catalog.insert(abs_str.clone(), catalog_count);
        }
        stats_by_catalog.insert(abs_str, counts);
    }

    // Sort: by catalog_path then unit_id (both strings, BTreeMap already gave
    // catalog order; within each catalog units came in document order — resort
    // by unit_id for stable output).
    items.sort_by(|a, b| {
        a.catalog_path
            .cmp(&b.catalog_path)
            .then_with(|| a.unit_id.cmp(&b.unit_id))
    });

    let total_count = items.len();

    Ok(ReviewQueueResponse {
        total_count,
        by_catalog,
        stats_by_catalog,
        items,
    })
}

/// Truncate `s` to at most `max_chars` characters, preferring a word boundary.
///
/// `cutoff` and `search_start` are byte indices derived from `char_indices`,
/// so they always land on UTF-8 char boundaries — slicing the string between
/// them never panics on multi-byte text. (Codex P1 on PR #33: a previous
/// implementation derived `search_start` via byte subtraction, which crashed
/// `scan_project_review_state` on long non-ASCII previews.)
pub(crate) fn truncate_preview(s: &str, max_chars: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() <= max_chars {
        return s;
    }
    let cutoff = s
        .char_indices()
        .nth(max_chars)
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    // Try to break at a word boundary within the last 20 chars of the limit.
    let search_start_char = max_chars.saturating_sub(20);
    let search_start = s
        .char_indices()
        .nth(search_start_char)
        .map(|(i, _)| i)
        .unwrap_or(0);
    let best = s[search_start..cutoff]
        .rfind(' ')
        .map(|off| search_start + off)
        .unwrap_or(cutoff);
    format!("{}…", s[..best].trim_end())
}

/// Extract a plain-text preview from a `Target`, truncated to `max_chars`.
pub(crate) fn extract_target_text(target: &i18n_harness_core::Target, max_chars: usize) -> String {
    match target {
        i18n_harness_core::Target::Singular { text: Some(t) } => truncate_preview(t, max_chars),
        i18n_harness_core::Target::Plural { forms } => {
            // Use the first non-None form as the preview.
            forms
                .iter()
                .flatten()
                .next()
                .map(|t| truncate_preview(t, max_chars))
                .unwrap_or_default()
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod truncate_preview_tests {
    use super::truncate_preview;

    #[test]
    fn ascii_under_limit_returns_unchanged() {
        assert_eq!(truncate_preview("hello world", 100), "hello world");
    }

    #[test]
    fn ascii_over_limit_breaks_at_word() {
        let out = truncate_preview("the quick brown fox jumps over the lazy dog", 20);
        assert!(out.ends_with('…'));
        assert!(out.len() <= 25);
    }

    #[test]
    fn multibyte_text_does_not_panic_anywhere_near_cutoff() {
        // Long Cyrillic/CJK strings where every char is 2-3 bytes —
        // the previous byte-subtraction crashed somewhere in here.
        let cyr: String = "Здравствуйте мир ".repeat(20);
        let _ = truncate_preview(&cyr, 50);
        let cjk: String = "你好世界今天天气真好".repeat(20);
        let _ = truncate_preview(&cjk, 50);
        let mixed = format!("hello {} world {}", cyr, cjk);
        let _ = truncate_preview(&mixed, 50);
    }

    #[test]
    fn newlines_collapse_to_spaces() {
        let out = truncate_preview("line one\nline two\nline three\nline four", 20);
        assert!(!out.contains('\n'));
    }
}
