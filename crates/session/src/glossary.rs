//! The project glossary as a translator edits it: one list of terms, each
//! either kept unchanged or with a fixed translation.

use std::path::PathBuf;

use i18n_harness_glossary::{Glossary, Term};
use i18n_harness_project::{GlossaryConfig, Project};
use serde::{Deserialize, Serialize};

use i18n_harness_gate::{FindingDetail, term_occurs};

use crate::error::{Result, SessionError};
use crate::session::{Session, is_active};

/// What [`Session::import_glossary`] changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GlossaryImport {
    /// Terms the glossary did not have.
    pub added: usize,
    /// Terms it had, now replaced by the imported row.
    pub updated: usize,
}

/// How the strings of one target language use a glossary term.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TermUsage {
    /// The term as it appears in the original text.
    pub term: String,
    /// Strings whose original contains the term.
    pub strings: usize,
    /// Translations that do not follow the term: a kept term changed, or a
    /// fixed translation missing.
    pub breaking: usize,
}

/// Default file name of a glossary the harness creates.
const GLOSSARY_FILE: &str = "glossary.toml";

/// One glossary term, seen from one target language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlossaryEntry {
    /// The term as it appears in the original text.
    pub term: String,
    /// Never translate it (product names, identifiers, units).
    pub keep: bool,
    /// The required translation into the language asked for.
    #[serde(default)]
    pub translation: Option<String>,
    /// Free-form note for translators.
    #[serde(default)]
    pub note: Option<String>,
}

impl Session {
    /// For every glossary term, how many strings of the catalogs in `locale`
    /// use it and how many translations break it, in glossary order.
    pub fn glossary_usage(&self, locale: &str) -> Vec<TermUsage> {
        let Some(glossary) = self.project.glossary() else {
            return Vec::new();
        };
        let terms: Vec<(&str, bool)> = glossary
            .terms()
            .map(|(source, term)| (source, term.do_not_translate))
            .collect();
        let mut usage: Vec<TermUsage> = terms
            .iter()
            .map(|(term, _)| TermUsage {
                term: (*term).to_owned(),
                strings: 0,
                breaking: 0,
            })
            .collect();
        for catalog in self.catalogs.iter().filter(|c| c.locale.id == locale) {
            for unit in catalog.doc.units().iter().filter(|u| is_active(u)) {
                let used: Vec<usize> = terms
                    .iter()
                    .enumerate()
                    .filter(|(_, (term, keep))| term_occurs(&unit.source, term, *keep))
                    .map(|(i, _)| i)
                    .collect();
                if used.is_empty() {
                    continue;
                }
                let report = catalog
                    .is_translation(unit)
                    .then(|| catalog.gate(unit, Some(glossary)));
                for i in used {
                    usage[i].strings += 1;
                    let broken = report.as_ref().is_some_and(|r| {
                        r.findings.iter().any(|f| match &f.detail {
                            FindingDetail::ProtectedTermMismatch(d) => d.term == usage[i].term,
                            FindingDetail::TermTranslationMissing(d) => d.term == usage[i].term,
                            _ => false,
                        })
                    });
                    usage[i].breaking += usize::from(broken);
                }
            }
        }
        usage
    }

    /// The glossary as seen from `locale`, in alphabetical order.
    pub fn glossary(&self, locale: &str) -> Vec<GlossaryEntry> {
        self.project
            .glossary()
            .map(|g| {
                g.terms()
                    .map(|(source, term)| GlossaryEntry {
                        term: source.to_owned(),
                        keep: term.do_not_translate,
                        translation: term.translations.get(locale).cloned(),
                        note: term.notes.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Replace the glossary's terms with `entries`, setting translations for
    /// `locale` and keeping every other language's translations of the terms
    /// that remain. Creates `glossary.toml` in the project folder when the
    /// project has none.
    ///
    /// # Errors
    ///
    /// Failures writing the glossary or the settings file.
    pub fn set_glossary(&mut self, locale: &str, entries: Vec<GlossaryEntry>) -> Result<()> {
        let mut glossary = self
            .project
            .glossary()
            .cloned()
            .unwrap_or_else(Glossary::empty);
        let keep: std::collections::HashSet<String> =
            entries.iter().map(|e| e.term.trim().to_owned()).collect();
        let stale: Vec<String> = glossary
            .terms()
            .map(|(source, _)| source.to_owned())
            .filter(|source| !keep.contains(source))
            .collect();
        for source in stale {
            glossary.remove_term(&source);
        }
        for entry in entries {
            let source = entry.term.trim().to_owned();
            if source.is_empty() {
                continue;
            }
            let mut term = glossary.term(&source).cloned().unwrap_or(Term {
                do_not_translate: false,
                notes: None,
                translations: Default::default(),
            });
            term.do_not_translate = entry.keep;
            term.notes = entry.note.filter(|n| !n.trim().is_empty());
            match entry.translation.filter(|t| !t.trim().is_empty()) {
                Some(t) => {
                    term.translations.insert(locale.to_owned(), t);
                }
                None => {
                    term.translations.remove(locale);
                }
            }
            glossary.insert_term(source, term);
        }

        let toml = glossary.to_toml().map_err(|e| SessionError::Io {
            context: "cannot serialize the glossary".into(),
            source: std::io::Error::other(e.to_string()),
        })?;
        let path = match self.project.paths().glossary() {
            Some(path) => path.to_path_buf(),
            None => {
                self.project.set_glossary(GlossaryConfig {
                    path: PathBuf::from(GLOSSARY_FILE),
                })?;
                self.project.save_manifest()?;
                self.root().join(GLOSSARY_FILE)
            }
        };
        i18n_harness_core::fs::write_atomic(&path, toml.as_bytes()).map_err(|source| {
            SessionError::Io {
                context: format!("cannot write {}", path.display()),
                source,
            }
        })?;
        // The project caches its glossary; re-open it to pick up the file.
        let (project, _) = Project::open(self.root())?;
        self.project = project;
        Ok(())
    }
}

impl Session {
    /// Merge a term table into the glossary for `locale`: rows of
    /// `term, translation, note`, as a spreadsheet saves them (comma,
    /// semicolon or tab separated, quotes allowed; a header row starting
    /// `term`, `source` or `original` is skipped). A row without a
    /// translation, or with the term itself as translation, keeps the term
    /// unchanged. Imported rows replace existing terms of the same text.
    ///
    /// # Errors
    ///
    /// [`SessionError::InvalidInput`] for a malformed table; failures
    /// writing the glossary.
    pub fn import_glossary(&mut self, locale: &str, table: &str) -> Result<GlossaryImport> {
        let mut entries = self.glossary(locale);
        let mut summary = GlossaryImport::default();
        for entry in glossary_table_entries(table)? {
            match entries.iter_mut().find(|e| e.term == entry.term) {
                Some(existing) => {
                    *existing = entry;
                    summary.updated += 1;
                }
                None => {
                    entries.push(entry);
                    summary.added += 1;
                }
            }
        }
        self.set_glossary(locale, entries)?;
        Ok(summary)
    }
}

/// The glossary entries of a term table, in the shape
/// [`Session::import_glossary`] reads, without touching any glossary.
///
/// # Errors
///
/// [`SessionError::InvalidInput`] for a malformed table.
pub fn glossary_table_entries(table: &str) -> Result<Vec<GlossaryEntry>> {
    let rows = parse_table(table).map_err(SessionError::InvalidInput)?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let mut cells = row.into_iter().map(|c| c.trim().to_owned());
            let term = cells.next().filter(|t| !t.is_empty())?;
            let translation = cells.next().filter(|t| !t.is_empty() && *t != term);
            let note = cells.next().filter(|n| !n.is_empty());
            Some(GlossaryEntry {
                keep: translation.is_none(),
                term,
                translation,
                note,
            })
        })
        .collect())
}

/// Rows of a delimited table. The delimiter is whichever of tab, semicolon
/// and comma occurs most in the first lines (a first row may have a single
/// column).
fn parse_table(text: &str) -> std::result::Result<Vec<Vec<String>>, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let head: String = text.lines().take(20).collect();
    let delimiter = ['\t', ';', ',']
        .into_iter()
        .max_by_key(|d| head.matches(*d).count())
        .filter(|d| head.contains(*d))
        .unwrap_or(',');

    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    let mut line = 1;
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                cell.push('"');
            }
            '"' if quoted => quoted = false,
            '"' if cell.trim().is_empty() => {
                cell.clear();
                quoted = true;
            }
            '\n' if !quoted => {
                row.push(std::mem::take(&mut cell).trim_end_matches('\r').to_owned());
                rows.push(std::mem::take(&mut row));
                line += 1;
            }
            c if c == delimiter && !quoted => row.push(std::mem::take(&mut cell)),
            c => {
                if c == '\n' {
                    line += 1;
                }
                cell.push(c);
            }
        }
    }
    if quoted {
        return Err(format!("line {line}: a quoted cell is not closed"));
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows.retain(|r| r.iter().any(|c| !c.trim().is_empty()));
    let header = rows.first().and_then(|r| r.first()).is_some_and(|c| {
        matches!(
            c.trim().to_lowercase().as_str(),
            "term" | "source" | "original"
        )
    });
    if header {
        rows.remove(0);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::parse_table;

    #[test]
    fn reads_what_spreadsheets_save() {
        let rows = parse_table(
            "Term;Translation;Note\r\nThreshold;Prahová hodnota;\r\n\"Blob\";;keep\r\n\"Open, then close\";\"Otvoriť, potom \"\"zavrieť\"\"\";\r\n",
        )
        .unwrap();
        assert_eq!(
            rows,
            [
                vec!["Threshold", "Prahová hodnota", ""],
                vec!["Blob", "", "keep"],
                vec!["Open, then close", "Otvoriť, potom \"zavrieť\"", ""],
            ]
        );
    }

    #[test]
    fn tab_separated_without_header() {
        let rows = parse_table("ADS\nThreshold\tPrahová hodnota").unwrap();
        assert_eq!(rows, [vec!["ADS"], vec!["Threshold", "Prahová hodnota"]]);
    }

    #[test]
    fn an_unclosed_quote_is_an_error() {
        assert!(parse_table("\"Threshold;x\n").is_err());
    }
}
