//! [`Session`]: a project folder and its open catalogs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use i18n_harness_core::{ReviewStatus, Target, Unit, UnitId, UnitState};
use i18n_harness_formats::{FormatDoc, FormatRegistry};
use i18n_harness_gate::{GateContext, GateReport, validate_with};
use i18n_harness_glossary::Glossary;
use i18n_harness_locales::Locale;
use i18n_harness_project::{CatalogRef, Project, ProjectWarning};

use crate::error::{Result, SessionError};
use crate::glossary::term_hits;
use crate::issues::issues;
use crate::shared::{SharedIndex, SharedInfo};
use crate::view::{
    CatalogSummary, CheckSettings, IssueSeverity, PluralSlot, SaveReport, Show, StatusCounts,
    UnitFilter, UnitPage, UnitRow, UnitStatus, UnitView,
};

/// One open catalog.
#[derive(Debug)]
pub(crate) struct OpenCatalog {
    pub(crate) path: PathBuf,
    pub(crate) relative: String,
    pub(crate) doc: Box<dyn FormatDoc>,
    /// The catalog's locale with the project's overrides applied.
    pub(crate) locale: Locale,
    /// Language of the text the targets held when opened, if not `locale`.
    pub(crate) pivot: Option<&'static Locale>,
    /// Non-empty targets as they were when the catalog was opened.
    pub(crate) existing: HashMap<UnitId, Target>,
    /// States as they were when the catalog was opened, for strings with
    /// [`Self::existing`] text.
    pub(crate) existing_state: HashMap<UnitId, UnitState>,
    /// Review statuses as they were when the catalog was opened.
    pub(crate) existing_review: HashMap<UnitId, Option<ReviewStatus>>,
    pub(crate) dirty: bool,
}

impl OpenCatalog {
    pub(crate) fn gate(&self, unit: &Unit, glossary: Option<&Glossary>) -> GateReport {
        validate_with(
            unit,
            &GateContext {
                locale: &self.locale,
                glossary,
                plural_forms: self.doc.plural_forms(),
            },
        )
    }

    /// Whether the unit's target is a translation into this catalog's
    /// language that can be judged: it has text, and in a pivot catalog it
    /// is not just the unedited pivot-language input.
    pub(crate) fn is_translation(&self, unit: &Unit) -> bool {
        // Text identical to the pivot input is still a translation once a
        // model or a person produced it (`mm<sup>2</sup>`, `<HEADER>`): the
        // review store says so.
        let reviewed = unit.review_status.is_some() && !unit.source_changed_since_review;
        has_target_text(&unit.target)
            && (self.pivot.is_none()
                || reviewed
                || self.existing.get(&unit.id) != Some(&unit.target))
    }

    pub(crate) fn index_of(&self, id: &UnitId) -> Result<usize> {
        self.doc
            .units()
            .iter()
            .position(|u| u.id == *id)
            .ok_or_else(|| SessionError::UnknownUnit {
                path: self.path.clone(),
                id: id.0.clone(),
            })
    }
}

/// A project folder with its catalogs open in memory.
///
/// Every edit stays in memory until [`Self::save`]. Review decisions (a
/// string confirmed as done, a machine translation awaiting a check) are
/// recorded in the project's state folder as they happen, so they survive a
/// restart even before the catalog is saved.
#[derive(Debug)]
pub struct Session {
    pub(crate) project: Project,
    pub(crate) catalogs: Vec<OpenCatalog>,
    warnings: Vec<String>,
}

impl Session {
    /// Open the translation files in `root`.
    ///
    /// A folder that already has an `i18n-harness.toml` opens as that project.
    /// Otherwise the folder is scanned for catalog files and the settings file
    /// is created from what was found (format and language are detected from
    /// the files themselves).
    ///
    /// # Errors
    ///
    /// [`SessionError::Project`] when the folder holds no translation files
    /// or its settings file is invalid; [`SessionError::Catalog`] when a file
    /// cannot be parsed.
    pub fn open_folder(root: &Path) -> Result<Self> {
        let (project, warnings) = if root.join("i18n-harness.toml").is_file() {
            Project::open(root)?
        } else {
            let draft = Project::discover(root)?;
            if draft.catalogs.is_empty() {
                return Err(SessionError::Io {
                    context: format!("no translation files found in {}", root.display()),
                    source: std::io::Error::new(std::io::ErrorKind::NotFound, "no catalogs"),
                });
            }
            Project::create_from_draft(root, draft)?
        };
        Self::from_project(project, &warnings)
    }

    fn from_project(project: Project, warnings: &[ProjectWarning]) -> Result<Self> {
        let mut catalogs = Vec::new();
        let refs: Vec<CatalogRef> = project.catalogs().to_vec();
        for catalog in &refs {
            catalogs.push(open_catalog(&project, catalog)?);
        }
        Ok(Self {
            project,
            catalogs,
            warnings: warnings.iter().map(ToString::to_string).collect(),
        })
    }

    /// The project folder.
    pub fn root(&self) -> &Path {
        self.project.paths().root()
    }

    /// The underlying project (settings, glossary, review store).
    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Mutable access to the project, for settings changes. Call
    /// [`Self::reload`] after changes that affect how catalogs are read
    /// (locale overrides, pivot languages, catalogs added or removed).
    pub fn project_mut(&mut self) -> &mut Project {
        &mut self.project
    }

    /// Problems found while opening, in plain words.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Re-read every catalog from disk, dropping unsaved edits.
    ///
    /// # Errors
    ///
    /// As [`Self::open_folder`].
    pub fn reload(&mut self) -> Result<()> {
        let refs: Vec<CatalogRef> = self.project.catalogs().to_vec();
        let mut catalogs = Vec::new();
        for catalog in &refs {
            catalogs.push(open_catalog(&self.project, catalog)?);
        }
        self.catalogs = catalogs;
        Ok(())
    }

    /// Set or clear the language the catalog's existing targets are in, save
    /// the settings, and re-open that catalog.
    ///
    /// # Errors
    ///
    /// [`SessionError::UnknownCatalog`], [`SessionError::UnknownLocale`], or
    /// a failure writing the settings file.
    pub fn set_pivot(&mut self, path: &Path, pivot: Option<&str>) -> Result<()> {
        if let Some(id) = pivot {
            Locale::by_id(id).ok_or_else(|| SessionError::UnknownLocale(id.to_owned()))?;
        }
        let relative = PathBuf::from(&self.catalog(path)?.relative);
        self.project
            .set_catalog_pivot(&relative, pivot.map(str::to_owned))?;
        self.project.save_manifest()?;
        let catalog_ref = self
            .project
            .catalogs()
            .iter()
            .find(|c| Path::new(&c.absolute_path) == path)
            .cloned()
            .ok_or_else(|| SessionError::UnknownCatalog {
                path: path.to_path_buf(),
            })?;
        let reopened = open_catalog(&self.project, &catalog_ref)?;
        let slot = self.catalog_index(path)?;
        self.catalogs[slot] = reopened;
        Ok(())
    }

    /// Summaries of every open catalog, in settings order.
    pub fn catalogs(&self) -> Vec<CatalogSummary> {
        let shared = SharedIndex::build(&self.catalogs);
        (0..self.catalogs.len())
            .map(|slot| self.summarize(slot, &shared))
            .collect()
    }

    /// Summary of one catalog.
    ///
    /// # Errors
    ///
    /// [`SessionError::UnknownCatalog`].
    pub fn catalog_summary(&self, path: &Path) -> Result<CatalogSummary> {
        let slot = self.catalog_index(path)?;
        Ok(self.summarize(slot, &SharedIndex::build(&self.catalogs)))
    }

    fn summarize(&self, slot: usize, shared: &SharedIndex) -> CatalogSummary {
        let c = &self.catalogs[slot];
        let glossary = self.project.glossary();
        let mut counts = StatusCounts::default();
        for (index, unit) in c.doc.units().iter().enumerate() {
            if !is_active(unit) {
                continue;
            }
            counts.total += 1;
            match status_of(unit, c.pivot.is_some()) {
                UnitStatus::ToDo => counts.to_do += 1,
                UnitStatus::Check => counts.check += 1,
                UnitStatus::Done => counts.done += 1,
            }
            let report = c.is_translation(unit).then(|| c.gate(unit, glossary));
            if let Some(report) = &report {
                let found = issues(report, unit, c.doc.format());
                let has = |s| found.iter().any(|i| i.severity == s);
                counts.errors += usize::from(has(IssueSeverity::Error));
                counts.warnings += usize::from(has(IssueSeverity::Warning));
            }
            let hits = term_hits(glossary, c, unit, report.as_ref());
            counts.glossary += usize::from(!hits.is_empty());
            counts.glossary_broken += usize::from(hits.iter().any(|h| h.broken));
            let info = shared.info(&self.catalogs, (slot, index));
            counts.only_here += usize::from(!info.shared);
            counts.inconsistent += usize::from(info.inconsistent);
        }
        CatalogSummary {
            path: c.path.display().to_string(),
            name: file_name(&c.path),
            relative_path: c.relative.clone(),
            format: c.doc.format().id().to_owned(),
            format_name: c.doc.format().display_name().to_owned(),
            locale: c.locale.id.to_owned(),
            language: c.locale.name.to_owned(),
            pivot: c.pivot.map(|p| p.id.to_owned()),
            pivot_language: c.pivot.map(|p| p.name.to_owned()),
            counts,
            dirty: c.dirty,
        }
    }

    /// The strings of one catalog that match `filter`, `limit` of them
    /// starting at `offset`, in document order.
    ///
    /// # Errors
    ///
    /// [`SessionError::UnknownCatalog`].
    pub fn units(
        &self,
        path: &Path,
        filter: &UnitFilter,
        offset: usize,
        limit: usize,
    ) -> Result<UnitPage> {
        let slot = self.catalog_index(path)?;
        let selected = self.select(&[slot], filter);
        let matching = selected.len();
        let units = selected
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|(_, view, _)| view)
            .collect();
        Ok(UnitPage { units, matching })
    }

    /// Every string matching `filter`, of one catalog (`path`) or of all,
    /// as list rows in catalog then document order.
    ///
    /// # Errors
    ///
    /// [`SessionError::UnknownCatalog`].
    pub fn rows(&self, path: Option<&Path>, filter: &UnitFilter) -> Result<Vec<UnitRow>> {
        let slots: Vec<usize> = match path {
            Some(path) => vec![self.catalog_index(path)?],
            None => (0..self.catalogs.len()).collect(),
        };
        Ok(self
            .select(&slots, filter)
            .into_iter()
            .map(|(slot, view, info)| {
                let catalog = &self.catalogs[slot];
                let has = |s| view.issues.iter().any(|i| i.severity == s);
                UnitRow {
                    path: catalog.path.display().to_string(),
                    file: file_name(&catalog.path),
                    error: has(IssueSeverity::Error),
                    warning: has(IssueSeverity::Warning),
                    shared: info.shared,
                    inconsistent: info.inconsistent,
                    terms: view.terms.len(),
                    glossary_broken: view.terms.iter().any(|t| t.broken),
                    group: view.context.group.clone(),
                    existing: view.existing.and_then(|e| e.into_iter().next()),
                    target: view.target.into_iter().next().unwrap_or_default(),
                    id: view.id,
                    source: view.source,
                    status: view.status,
                }
            })
            .collect())
    }

    /// The strings of the catalogs in `slots` that match `filter`, with
    /// their views and how they relate to other files.
    fn select(&self, slots: &[usize], filter: &UnitFilter) -> Vec<(usize, UnitView, SharedInfo)> {
        let shared = SharedIndex::build(&self.catalogs);
        let needle = filter
            .search
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_lowercase);
        let mut out = Vec::new();
        for &slot in slots {
            let catalog = &self.catalogs[slot];
            for (index, unit) in catalog.doc.units().iter().enumerate() {
                if !is_active(unit) {
                    continue;
                }
                let view = self.view(catalog, unit);
                if !filter.statuses.is_empty() && !filter.statuses.contains(&view.status) {
                    continue;
                }
                let info = shared.info(&self.catalogs, (slot, index));
                let has = |s| view.issues.iter().any(|i| i.severity == s);
                let shown = match filter.show {
                    Show::Everything => true,
                    Show::Problems => has(IssueSeverity::Error),
                    Show::Warnings => has(IssueSeverity::Warning),
                    Show::Inconsistent => info.inconsistent,
                    Show::Shared => info.shared,
                    Show::Glossary => !view.terms.is_empty(),
                    Show::GlossaryBroken => view.terms.iter().any(|t| t.broken),
                };
                if !shown {
                    continue;
                }
                if let Some(term) = &filter.term
                    && !view.terms.iter().any(|t| &t.term == term)
                {
                    continue;
                }
                if let Some(needle) = &needle
                    && !matches_search(&view, needle)
                {
                    continue;
                }
                out.push((slot, view, info));
            }
        }
        out
    }

    /// The tunable check settings for target `locale`, with this project's
    /// overrides applied.
    ///
    /// # Errors
    ///
    /// [`SessionError::UnknownLocale`].
    pub fn check_settings(&self, locale: &str) -> Result<CheckSettings> {
        let effective = self
            .project
            .locale(locale)
            .map(|r| r.effective_locale())
            .or_else(|| Locale::by_id(locale).copied())
            .ok_or_else(|| SessionError::UnknownLocale(locale.to_owned()))?;
        Ok(CheckSettings {
            length_ratio: effective.length_warn_ratio,
            length_min_chars: effective.length_warn_min_chars,
        })
    }

    /// Store the check settings for target `locale` in the settings file and
    /// apply them to the open catalogs of that language.
    ///
    /// # Errors
    ///
    /// [`SessionError::InvalidInput`] for a ratio that is not a positive
    /// number; [`SessionError::Project`] when the settings file cannot be
    /// written.
    pub fn set_check_settings(&mut self, locale: &str, settings: CheckSettings) -> Result<()> {
        if !(settings.length_ratio.is_finite() && settings.length_ratio > 0.0) {
            return Err(SessionError::InvalidInput(format!(
                "the length ratio must be a positive number, not {}",
                settings.length_ratio
            )));
        }
        let mut config = self
            .project
            .manifest()
            .locales
            .get(locale)
            .cloned()
            .unwrap_or_default();
        config.length_warn_ratio = Some(settings.length_ratio);
        config.length_warn_min_chars = Some(settings.length_min_chars);
        self.project.update_locale(locale, config)?;
        self.project.save_manifest()?;
        let effective = self.check_settings(locale)?;
        for catalog in self.catalogs.iter_mut().filter(|c| c.locale.id == locale) {
            catalog.locale.length_warn_ratio = effective.length_ratio;
            catalog.locale.length_warn_min_chars = effective.length_min_chars;
        }
        Ok(())
    }

    /// One string.
    ///
    /// # Errors
    ///
    /// [`SessionError::UnknownCatalog`], [`SessionError::UnknownUnit`].
    pub fn unit(&self, path: &Path, id: &str) -> Result<UnitView> {
        let catalog = self.catalog(path)?;
        let unit = &catalog.doc.units()[catalog.index_of(&UnitId::from(id))?];
        Ok(self.view(catalog, unit))
    }

    pub(crate) fn view(&self, catalog: &OpenCatalog, unit: &Unit) -> UnitView {
        let format = catalog.doc.format();
        let native = |icu: &str| {
            format
                .placeholders_from_icu(icu)
                .unwrap_or_else(|_| icu.to_owned())
        };
        let forms = |target: &Target| -> Vec<String> {
            match target {
                Target::Singular { text } => vec![text.as_deref().map(native).unwrap_or_default()],
                Target::Plural { forms } => forms
                    .iter()
                    .map(|f| f.as_deref().map(native).unwrap_or_default())
                    .collect(),
            }
        };
        let report = catalog.gate(unit, self.project.glossary());
        let translation = catalog.is_translation(unit);
        let terms = term_hits(self.project.glossary(), catalog, unit, Some(&report));
        let issues = if translation {
            issues(&report, unit, format)
        } else {
            Vec::new()
        };
        // In a pivot catalog, untouched text is the input, not a translation:
        // show it as `existing` only, with an empty translation.
        let target = if catalog.pivot.is_some() && !translation {
            vec![String::new(); forms(&unit.target).len()]
        } else {
            forms(&unit.target)
        };
        UnitView {
            id: unit.id.0.clone(),
            source: native(&unit.source),
            existing: catalog.existing.get(&unit.id).map(forms),
            target,
            plural_slots: plural_slots(catalog, unit),
            status: status_of(unit, catalog.pivot.is_some()),
            state: unit.state,
            issues,
            terms,
            context: unit.context.clone(),
            editable: unit.state.is_ui_editable(),
        }
    }

    /// Replace a string's translation. `forms` are in the catalog's native
    /// placeholder syntax, one per plural form (one for a non-plural string).
    /// `done` marks the translation as confirmed; otherwise it awaits a check.
    ///
    /// # Errors
    ///
    /// [`SessionError::UnknownUnit`], [`SessionError::NotEditable`],
    /// [`SessionError::Placeholder`] when a form uses syntax the format
    /// cannot store.
    pub fn set_translation(
        &mut self,
        path: &Path,
        id: &str,
        forms: Vec<String>,
        done: bool,
    ) -> Result<UnitView> {
        let slot = self.catalog_index(path)?;
        let catalog = &mut self.catalogs[slot];
        let index = catalog.index_of(&UnitId::from(id))?;
        let format = catalog.doc.format();
        let unit = &mut catalog.doc.units_mut()[index];
        if !unit.state.is_ui_editable() {
            return Err(SessionError::NotEditable { id: id.to_owned() });
        }
        let to_icu = |text: String| -> Result<Option<String>> {
            if text.is_empty() {
                Ok(None)
            } else {
                Ok(Some(format.placeholders_to_icu(&text)?))
            }
        };
        unit.target = match &unit.target {
            Target::Singular { .. } => Target::Singular {
                text: to_icu(forms.into_iter().next().unwrap_or_default())?,
            },
            Target::Plural { .. } => Target::Plural {
                forms: forms.into_iter().map(to_icu).collect::<Result<_>>()?,
            },
        };
        // A human wrote this; the model's remarks about its own earlier
        // output no longer apply.
        unit.flags = Default::default();
        unit.flag_notes.clear();
        unit.confidence = None;
        catalog.dirty = true;
        self.mark(slot, index, done)
    }

    /// Mark strings as done (confirmed) or as awaiting a check.
    ///
    /// # Errors
    ///
    /// [`SessionError::UnknownUnit`] for an id not in the catalog; nothing is
    /// changed in that case.
    pub fn set_done(&mut self, path: &Path, ids: &[String], done: bool) -> Result<Vec<UnitView>> {
        let slot = self.catalog_index(path)?;
        let indices: Vec<usize> = ids
            .iter()
            .map(|id| self.catalogs[slot].index_of(&UnitId::from(id.as_str())))
            .collect::<Result<_>>()?;
        let mut views = Vec::with_capacity(indices.len());
        for index in indices {
            let unit = &self.catalogs[slot].doc.units()[index];
            if !unit.state.is_ui_editable() || !has_complete_text(&unit.target) {
                continue;
            }
            self.catalogs[slot].dirty = true;
            views.push(self.mark(slot, index, done)?);
        }
        Ok(views)
    }

    /// Record a string's state and review status after an edit or decision.
    pub(crate) fn mark(&mut self, slot: usize, index: usize, done: bool) -> Result<UnitView> {
        let review = if done {
            ReviewStatus::Approved
        } else {
            ReviewStatus::NeedsReview
        };
        {
            let unit = &mut self.catalogs[slot].doc.units_mut()[index];
            unit.state = if done && has_complete_text(&unit.target) {
                UnitState::Finished
            } else if !has_target_text(&unit.target) {
                UnitState::Untranslated
            } else {
                UnitState::Proposed
            };
        }
        self.record_review(slot, index, review)?;
        let catalog = &self.catalogs[slot];
        Ok(self.view(catalog, &catalog.doc.units()[index]))
    }

    pub(crate) fn record_review(
        &mut self,
        slot: usize,
        index: usize,
        status: ReviewStatus,
    ) -> Result<()> {
        let path = self.catalogs[slot].path.clone();
        let unit = &mut self.catalogs[slot].doc.units_mut()[index];
        unit.review_status = Some(status);
        unit.source_changed_since_review = false;
        let hash = unit.source_hash.clone().unwrap_or_default();
        let id = unit.id.clone();
        self.project
            .set_review_status(&path, &id, Some(status), hash, None)?;
        Ok(())
    }

    /// Write catalogs with unsaved edits (`path`: just that one; `None`:
    /// all). Strings whose translation has an error that would break the
    /// application keep their previous text on disk; they stay edited in
    /// memory so they can be fixed.
    ///
    /// # Errors
    ///
    /// [`SessionError::Catalog`] when a file cannot be written; catalogs
    /// written before the failure stay written.
    pub fn save(&mut self, path: Option<&Path>) -> Result<SaveReport> {
        let glossary = self.project.glossary();
        let mut report = SaveReport::default();
        // The review store must describe what is on disk: a held-back
        // string's record goes back to what it was when opened.
        let mut restore: Vec<(PathBuf, UnitId, Option<ReviewStatus>, String)> = Vec::new();
        for catalog in &mut self.catalogs {
            if !catalog.dirty || path.is_some_and(|p| p != catalog.path) {
                continue;
            }
            let units: Vec<Unit> = catalog
                .doc
                .units()
                .iter()
                .map(|unit| {
                    // Only an edit can be held back; text the file already
                    // had is written as it was.
                    let edited = catalog.existing.get(&unit.id) != Some(&unit.target)
                        && has_target_text(&unit.target);
                    let blocked = unit.state.is_ui_editable()
                        && edited
                        && catalog.gate(unit, glossary).flags.has_hard();
                    if blocked {
                        report.held_back += 1;
                        restore.push((
                            catalog.path.clone(),
                            unit.id.clone(),
                            catalog.existing_review.get(&unit.id).copied().flatten(),
                            unit.source_hash.clone().unwrap_or_default(),
                        ));
                        held_back_version(catalog, unit)
                    } else {
                        unit.clone()
                    }
                })
                .collect();
            catalog.doc.write(&units, &catalog.path)?;
            // Re-open so the document's pristine snapshot matches the disk;
            // keep the in-memory edits that were held back.
            let mut fresh =
                FormatRegistry::builtin().open(&catalog.path, Some(catalog.doc.format().id()))?;
            let edited: HashMap<UnitId, Unit> = catalog
                .doc
                .units()
                .iter()
                .map(|u| (u.id.clone(), u.clone()))
                .collect();
            let mut still_dirty = false;
            for unit in fresh.units_mut() {
                if let Some(mine) = edited.get(&unit.id) {
                    if mine.target != unit.target {
                        still_dirty = true;
                    }
                    *unit = mine.clone();
                }
            }
            catalog.doc = fresh;
            catalog.dirty = still_dirty;
            report.written.push(catalog.path.display().to_string());
        }
        for (path, id, status, hash) in restore {
            self.project
                .set_review_status(&path, &id, status, hash, None)?;
        }
        Ok(report)
    }

    pub(crate) fn catalog(&self, path: &Path) -> Result<&OpenCatalog> {
        self.catalogs
            .iter()
            .find(|c| c.path == path)
            .ok_or_else(|| SessionError::UnknownCatalog {
                path: path.to_path_buf(),
            })
    }

    pub(crate) fn catalog_index(&self, path: &Path) -> Result<usize> {
        self.catalogs
            .iter()
            .position(|c| c.path == path)
            .ok_or_else(|| SessionError::UnknownCatalog {
                path: path.to_path_buf(),
            })
    }
}

fn open_catalog(project: &Project, catalog: &CatalogRef) -> Result<OpenCatalog> {
    let path = PathBuf::from(&catalog.absolute_path);
    let mut doc = FormatRegistry::builtin().open(&path, Some(catalog.format.as_str()))?;
    project.apply_review_state(&path, doc.units_mut());
    let locale = project
        .locale(&catalog.locale)
        .map(|r| r.effective_locale())
        .or_else(|| Locale::by_id(&catalog.locale).copied())
        .ok_or_else(|| SessionError::UnknownLocale(catalog.locale.clone()))?;
    let pivot = catalog
        .pivot
        .as_deref()
        .map(|id| Locale::by_id(id).ok_or_else(|| SessionError::UnknownLocale(id.to_owned())))
        .transpose()?;
    let opened = || doc.units().iter().filter(|u| has_target_text(&u.target));
    let existing = opened().map(|u| (u.id.clone(), u.target.clone())).collect();
    let existing_state = opened().map(|u| (u.id.clone(), u.state)).collect();
    let existing_review = doc
        .units()
        .iter()
        .map(|u| (u.id.clone(), u.review_status))
        .collect();
    Ok(OpenCatalog {
        path,
        relative: catalog.manifest_path.clone(),
        doc,
        locale,
        pivot,
        existing,
        existing_state,
        existing_review,
        dirty: false,
    })
}

/// The version of `unit` to write when its current translation is blocked:
/// the text the file had when opened, in the state it had.
fn held_back_version(catalog: &OpenCatalog, unit: &Unit) -> Unit {
    let mut original = unit.clone();
    match catalog.existing.get(&unit.id) {
        Some(target) => {
            original.target = target.clone();
            if let Some(state) = catalog.existing_state.get(&unit.id) {
                original.state = *state;
            }
        }
        None => {
            original.target = match &unit.target {
                Target::Singular { .. } => Target::Singular { text: None },
                Target::Plural { forms } => Target::Plural {
                    forms: vec![None; forms.len()],
                },
            };
            original.state = UnitState::Untranslated;
        }
    }
    original
}

pub(crate) fn is_active(unit: &Unit) -> bool {
    !matches!(unit.state, UnitState::Vanished | UnitState::Obsolete)
}

fn target_forms(target: &Target) -> Box<dyn Iterator<Item = Option<&str>> + '_> {
    match target {
        Target::Singular { text } => Box::new(std::iter::once(text.as_deref())),
        Target::Plural { forms } => Box::new(forms.iter().map(Option::as_deref)),
    }
}

/// Whether any form has text. An empty string is no text: Qt writes
/// `<translation></translation>` for untranslated messages that are not
/// marked unfinished.
pub(crate) fn has_target_text(target: &Target) -> bool {
    target_forms(target).any(|f| f.is_some_and(|s| !s.is_empty()))
}

/// Whether every form has text.
pub(crate) fn has_complete_text(target: &Target) -> bool {
    target_forms(target).all(|f| f.is_some_and(|s| !s.is_empty()))
}

/// Translator status of a string. In a catalog whose targets are in a pivot
/// language, a finished string nobody has reviewed still needs translating.
pub(crate) fn status_of(unit: &Unit, pivot: bool) -> UnitStatus {
    if is_active(unit) && !has_complete_text(&unit.target) {
        return UnitStatus::ToDo;
    }
    if !unit.source_changed_since_review {
        match unit.review_status {
            Some(ReviewStatus::Approved | ReviewStatus::Reviewed | ReviewStatus::Locked) => {
                return UnitStatus::Done;
            }
            Some(
                ReviewStatus::MachineTranslated
                | ReviewStatus::NeedsReview
                | ReviewStatus::Conflict,
            ) => return UnitStatus::Check,
            Some(ReviewStatus::Rejected) => return UnitStatus::ToDo,
            Some(ReviewStatus::New) | None => {}
        }
    }
    match unit.state {
        UnitState::Untranslated => UnitStatus::ToDo,
        // Unreviewed text in a pivot catalog is input, whatever its state.
        UnitState::Proposed | UnitState::Finished if pivot => UnitStatus::ToDo,
        UnitState::Proposed => UnitStatus::Check,
        UnitState::Finished => UnitStatus::Done,
        UnitState::Vanished | UnitState::Obsolete => UnitStatus::Done,
    }
}

fn plural_slots(catalog: &OpenCatalog, unit: &Unit) -> Vec<PluralSlot> {
    let Target::Plural { forms } = &unit.target else {
        return Vec::new();
    };
    let table = catalog.locale.integer_plural_forms;
    let labelled = forms.len() == table.len();
    (0..forms.len())
        .map(|i| PluralSlot {
            category: labelled.then(|| table[i].0.to_string()),
            example: labelled.then(|| table[i].1),
        })
        .collect()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn matches_search(view: &UnitView, needle: &str) -> bool {
    let hit = |s: &str| s.to_lowercase().contains(needle);
    hit(&view.source)
        || hit(&view.id)
        || view.target.iter().any(|t| hit(t))
        || view.existing.iter().flatten().any(|t| hit(t))
        || view.context.group.as_deref().is_some_and(hit)
        || view.context.developer_notes.iter().any(|n| hit(n))
}
