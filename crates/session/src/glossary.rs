//! The project glossary as a translator edits it: one list of terms, each
//! either kept unchanged or with a fixed translation.

use std::path::PathBuf;

use i18n_harness_glossary::{Glossary, Term};
use i18n_harness_project::{GlossaryConfig, Project};
use serde::{Deserialize, Serialize};

use crate::error::{Result, SessionError};
use crate::session::Session;

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
