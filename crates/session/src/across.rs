//! Moving translations between files that share strings: fill a string from
//! the files that already translate it, or copy one translation to every
//! file. Deterministic; no model is involved.

use std::path::{Path, PathBuf};

use i18n_harness_core::{Target, UnitId};

use crate::error::{Result, SessionError};
use crate::session::{Session, has_complete_text, is_active, status_of};
use crate::shared::{At, SharedIndex};
use crate::view::{FillReport, SiblingView, UnitStatus};

impl Session {
    /// The same string in the other files of its language.
    ///
    /// # Errors
    ///
    /// [`SessionError::UnknownCatalog`], [`SessionError::UnknownUnit`].
    pub fn siblings(&self, path: &Path, id: &str) -> Result<Vec<SiblingView>> {
        let at = self.at(path, id)?;
        let format = self.catalogs[at.0].doc.format();
        let native = |icu: &str| {
            format
                .placeholders_from_icu(icu)
                .unwrap_or_else(|_| icu.to_owned())
        };
        Ok(SharedIndex::build(&self.catalogs)
            .siblings(at)
            .into_iter()
            .map(|(slot, index)| {
                let catalog = &self.catalogs[slot];
                let unit = &catalog.doc.units()[index];
                let translated = catalog.is_translation(unit);
                let forms: Vec<Option<&str>> = match &unit.target {
                    Target::Singular { text } => vec![text.as_deref()],
                    Target::Plural { forms } => forms.iter().map(Option::as_deref).collect(),
                };
                SiblingView {
                    path: catalog.path.display().to_string(),
                    file: catalog
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    target: forms
                        .into_iter()
                        .map(|f| match f {
                            Some(text) if translated => native(text),
                            _ => String::new(),
                        })
                        .collect(),
                    status: status_of(unit, catalog.pivot.is_some()),
                }
            })
            .collect())
    }

    /// Copy the translation of a string to the same string in every other
    /// file of its language, with its status (done stays done). Returns how
    /// many strings changed.
    ///
    /// # Errors
    ///
    /// [`SessionError::UnknownCatalog`], [`SessionError::UnknownUnit`];
    /// [`SessionError::InvalidInput`] when the string has no complete
    /// translation to copy.
    pub fn use_everywhere(&mut self, path: &Path, id: &str) -> Result<usize> {
        let from = self.at(path, id)?;
        let catalog = &self.catalogs[from.0];
        let unit = &catalog.doc.units()[from.1];
        if !catalog.is_translation(unit) || !has_complete_text(&unit.target) {
            return Err(SessionError::InvalidInput(format!(
                "\"{id}\" has no complete translation to copy"
            )));
        }
        let done = status_of(unit, catalog.pivot.is_some()) == UnitStatus::Done;
        let siblings = SharedIndex::build(&self.catalogs).siblings(from);
        let mut changed = 0;
        for to in siblings {
            if self.copy_translation(from, to, done)? {
                changed += 1;
            }
        }
        Ok(changed)
    }

    /// Give each string to do in `paths` (every file when empty) the
    /// translation the other files of its language agree on, with the
    /// status it has there: done when one of them is done, else to check.
    /// Strings that other files translate differently are left alone and
    /// counted. With `apply` false nothing changes and the report says what
    /// would.
    ///
    /// # Errors
    ///
    /// [`SessionError::UnknownCatalog`] for a path not open.
    pub fn fill_from_other_files(&mut self, paths: &[PathBuf], apply: bool) -> Result<FillReport> {
        let slots: Vec<usize> = if paths.is_empty() {
            (0..self.catalogs.len()).collect()
        } else {
            paths
                .iter()
                .map(|p| self.catalog_index(p))
                .collect::<Result<_>>()?
        };
        let shared = SharedIndex::build(&self.catalogs);
        let mut report = FillReport::default();
        let mut plan: Vec<(At, At, bool)> = Vec::new();
        for slot in slots {
            let catalog = &self.catalogs[slot];
            for (index, unit) in catalog.doc.units().iter().enumerate() {
                if !is_active(unit)
                    || !unit.state.is_ui_editable()
                    || status_of(unit, catalog.pivot.is_some()) != UnitStatus::ToDo
                {
                    continue;
                }
                let sources: Vec<(At, &Target, bool)> = shared
                    .siblings((slot, index))
                    .into_iter()
                    .filter_map(|(s, i)| {
                        let other = &self.catalogs[s];
                        let u = &other.doc.units()[i];
                        (other.is_translation(u) && has_complete_text(&u.target)).then(|| {
                            let done = status_of(u, other.pivot.is_some()) == UnitStatus::Done;
                            ((s, i), &u.target, done)
                        })
                    })
                    .filter(|(_, target, _)| same_shape(target, &unit.target))
                    .collect();
                let Some(&(first, text, _)) = sources.first() else {
                    continue;
                };
                if sources.iter().any(|(_, t, _)| *t != text) {
                    report.conflicting += 1;
                    continue;
                }
                let done = sources.iter().any(|(_, _, d)| *d);
                let from = sources.iter().find(|(_, _, d)| *d).map_or(first, |s| s.0);
                plan.push((from, (slot, index), done));
                report.filled += 1;
            }
        }
        if apply {
            for (from, to, done) in plan {
                self.copy_translation(from, to, done)?;
            }
        }
        Ok(report)
    }

    /// Copy the translation at `from` onto `to` as an edit, marked done or
    /// to check. False when `to` cannot take it (not editable, or a
    /// different number of plural forms).
    fn copy_translation(&mut self, from: At, to: At, done: bool) -> Result<bool> {
        let target = self.catalogs[from.0].doc.units()[from.1].target.clone();
        {
            let unit = &mut self.catalogs[to.0].doc.units_mut()[to.1];
            if !unit.state.is_ui_editable() || !same_shape(&unit.target, &target) {
                return Ok(false);
            }
            unit.target = target;
            unit.flags = Default::default();
            unit.flag_notes.clear();
            unit.confidence = None;
        }
        self.catalogs[to.0].dirty = true;
        self.mark(to.0, to.1, done)?;
        Ok(true)
    }

    fn at(&self, path: &Path, id: &str) -> Result<At> {
        let slot = self.catalog_index(path)?;
        let index = self.catalogs[slot].index_of(&UnitId::from(id))?;
        Ok((slot, index))
    }
}

/// Whether two targets have the same form layout (one text, or the same
/// number of plural forms).
fn same_shape(a: &Target, b: &Target) -> bool {
    match (a, b) {
        (Target::Singular { .. }, Target::Singular { .. }) => true,
        (Target::Plural { forms: x }, Target::Plural { forms: y }) => x.len() == y.len(),
        _ => false,
    }
}
