//! Machine translation of a session's strings.
//!
//! A run is split so a caller can keep the session responsive while a model
//! works: [`Session::prepare`] copies what one string needs out of the
//! session, [`translate_task`] calls the model with no session borrowed, and
//! [`Session::apply`] merges the result back. [`Session::translate`] chains
//! the three for callers that can hold the session for the whole run.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use i18n_harness_backend::{TranslatedText, TranslationBackend, TranslationOutcome};
use i18n_harness_core::{Batch, BatchKey, FlagSet, ReviewStatus, Target, Unit, UnitId, UnitState};
use i18n_harness_glossary::Glossary;
use i18n_harness_locales::Locale;
use serde::{Deserialize, Serialize};

use crate::cancel::CancelToken;
use crate::error::{Result, SessionError};
use crate::session::{Session, has_complete_text, is_active, status_of};
use crate::view::{UnitStatus, UnitView};

/// Which strings to translate.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslateScope {
    /// Catalogs to cover (all open catalogs when empty).
    #[serde(default)]
    pub catalogs: Vec<PathBuf>,
    /// Statuses to cover ([`UnitStatus::ToDo`] when empty).
    #[serde(default)]
    pub statuses: Vec<UnitStatus>,
    /// Only these string ids (every matching string when `None`).
    #[serde(default)]
    pub ids: Option<Vec<String>>,
}

/// What text the model translates from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InputText {
    /// The existing translation when the catalog has a pivot language and
    /// that text is sound, else the original text.
    #[default]
    Auto,
    /// Always the original text.
    Source,
}

/// What a finished machine translation becomes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Promotion {
    /// Awaits a human check (status [`UnitStatus::Check`]).
    #[default]
    Review,
    /// Counts as done when it has no warnings and no errors. For unattended
    /// runs whose output is delivered as is.
    Deliver,
}

/// Everything needed to translate one string, owned so the session need not
/// stay borrowed while the model works.
#[derive(Debug, Clone)]
pub struct TranslationTask {
    /// Catalog the string belongs to.
    pub catalog: PathBuf,
    /// The string's id.
    pub id: UnitId,
    /// The unit as sent to the model: its source is the input text.
    pub request: Unit,
    /// Locale id of the input text.
    pub input_locale: String,
    /// Target locale (project overrides applied).
    pub locale: Locale,
    /// Project glossary.
    pub glossary: Option<Glossary>,
}

impl TranslationTask {
    /// Key under which identical requests share one model answer.
    pub fn memo_key(&self) -> MemoKey {
        MemoKey {
            input_locale: self.input_locale.clone(),
            locale: self.locale.id.to_owned(),
            text: self.request.source.clone(),
            plural: self.request.plural_arity.is_some(),
        }
    }
}

/// Identity of a model request for de-duplication.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MemoKey {
    input_locale: String,
    locale: String,
    text: String,
    plural: bool,
}

/// Model answers already obtained in a run, so a text that occurs many
/// times is translated once.
#[derive(Debug, Default)]
pub struct Memo(HashMap<MemoKey, TranslationOutcome>);

impl Memo {
    /// An empty memo.
    pub fn new() -> Self {
        Self::default()
    }

    /// The answer for `task`, asking `backend` only when the same request has
    /// not been answered before. Only successful answers are remembered.
    ///
    /// # Errors
    ///
    /// The backend's error.
    pub fn translate(
        &mut self,
        backend: &dyn TranslationBackend,
        task: &TranslationTask,
    ) -> Result<(TranslationOutcome, bool)> {
        let key = task.memo_key();
        if let Some(hit) = self.0.get(&key) {
            return Ok((hit.clone(), true));
        }
        let outcome = translate_task(backend, task)?;
        if matches!(outcome, TranslationOutcome::Translated { .. }) {
            self.0.insert(key, outcome.clone());
        }
        Ok((outcome, false))
    }
}

/// Ask `backend` to translate one prepared string.
///
/// # Errors
///
/// [`SessionError::Backend`] when the server cannot be reached or rejects
/// the request.
pub fn translate_task(
    backend: &dyn TranslationBackend,
    task: &TranslationTask,
) -> Result<TranslationOutcome> {
    let batch = Batch::new(
        BatchKey::new(task.catalog.display().to_string(), 0),
        vec![task.request.clone()],
    );
    let mut outcomes = backend.translate_batch(&batch, &task.locale, task.glossary.as_ref())?;
    Ok(outcomes.pop().unwrap_or(TranslationOutcome::Failed {
        reason: "the model returned no answer".into(),
        retryable: true,
        failure_kind: Default::default(),
    }))
}

/// What [`Session::apply`] did with one answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Applied {
    /// The string after the merge.
    pub unit: UnitView,
    /// Whether the string received a new translation.
    pub translated: bool,
    /// Why not, when it did not.
    pub failure: Option<String>,
}

/// Progress of a [`Session::translate`] run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum Progress {
    /// The run starts with `total` strings.
    Started {
        /// Strings in scope.
        total: usize,
    },
    /// One string is done (translated or failed).
    Unit {
        /// Catalog path.
        catalog: String,
        /// String id.
        id: String,
        /// Strings handled so far.
        done: usize,
        /// Strings in scope.
        total: usize,
        /// Whether it was translated.
        translated: bool,
        /// Why not, when it was not.
        failure: Option<String>,
    },
    /// The run ended.
    Finished {
        /// Strings translated.
        translated: usize,
        /// Strings the model failed on.
        failed: usize,
        /// Whether the run was cancelled before the end.
        cancelled: bool,
    },
}

impl Session {
    /// The strings `scope` covers, in catalog and document order.
    ///
    /// # Errors
    ///
    /// [`SessionError::UnknownCatalog`] for a catalog not open.
    pub fn plan(&self, scope: &TranslateScope) -> Result<Vec<(PathBuf, UnitId)>> {
        let statuses = if scope.statuses.is_empty() {
            vec![UnitStatus::ToDo]
        } else {
            scope.statuses.clone()
        };
        for path in &scope.catalogs {
            self.catalog(path)?;
        }
        let mut plan = Vec::new();
        for catalog in &self.catalogs {
            if !scope.catalogs.is_empty() && !scope.catalogs.contains(&catalog.path) {
                continue;
            }
            for unit in catalog.doc.units().iter().filter(|u| is_active(u)) {
                if !statuses.contains(&status_of(unit, catalog.pivot.is_some())) {
                    continue;
                }
                if let Some(ids) = &scope.ids
                    && !ids.contains(&unit.id.0)
                {
                    continue;
                }
                plan.push((catalog.path.clone(), unit.id.clone()));
            }
        }
        Ok(plan)
    }

    /// Copy out what translating one string needs.
    ///
    /// # Errors
    ///
    /// [`SessionError::UnknownCatalog`], [`SessionError::UnknownUnit`],
    /// [`SessionError::NotEditable`].
    pub fn prepare(&self, path: &Path, id: &UnitId, input: InputText) -> Result<TranslationTask> {
        let catalog = self.catalog(path)?;
        let unit = catalog
            .doc
            .units()
            .iter()
            .find(|u| u.id == *id)
            .ok_or_else(|| SessionError::UnknownUnit {
                path: path.to_path_buf(),
                id: id.0.clone(),
            })?;
        if !unit.state.is_ui_editable() {
            return Err(SessionError::NotEditable { id: id.0.clone() });
        }

        let mut request = unit.clone();
        request.target = empty_like(&unit.target);
        request.flags = FlagSet::new();
        request.flag_notes.clear();
        request.confidence = None;
        let mut input_locale = "en".to_owned();
        if let (InputText::Auto, Some(pivot), Some(existing)) =
            (input, catalog.pivot, catalog.existing.get(id))
            && let Some(text) = sound_pivot_text(catalog, unit, existing, self.project().glossary())
        {
            request.source = text;
            input_locale = pivot.id.to_owned();
        }

        Ok(TranslationTask {
            catalog: catalog.path.clone(),
            id: id.clone(),
            request,
            input_locale,
            locale: catalog.locale,
            glossary: self.project().glossary().cloned(),
        })
    }

    /// Merge a model answer into the session.
    ///
    /// # Errors
    ///
    /// [`SessionError::UnknownCatalog`], [`SessionError::UnknownUnit`], or a
    /// failure recording the review status.
    pub fn apply(
        &mut self,
        task: &TranslationTask,
        outcome: TranslationOutcome,
        promotion: Promotion,
    ) -> Result<Applied> {
        let slot = self.catalog_index(&task.catalog)?;
        let index = self.catalogs[slot]
            .doc
            .units()
            .iter()
            .position(|u| u.id == task.id)
            .ok_or_else(|| SessionError::UnknownUnit {
                path: task.catalog.clone(),
                id: task.id.0.clone(),
            })?;

        let (text, flags, confidence, flag_notes) = match outcome {
            TranslationOutcome::Translated {
                text,
                flags,
                confidence,
                flag_notes,
            } => (text, flags, confidence, flag_notes),
            TranslationOutcome::Skipped { reason } | TranslationOutcome::Failed { reason, .. } => {
                let catalog = &self.catalogs[slot];
                return Ok(Applied {
                    unit: self.view(catalog, &catalog.doc.units()[index]),
                    translated: false,
                    failure: Some(reason),
                });
            }
        };

        let mut candidate = self.catalogs[slot].doc.units()[index].clone();
        candidate.target = match text {
            TranslatedText::Singular(s) => Target::Singular { text: Some(s) },
            TranslatedText::Plural(forms) => Target::Plural {
                forms: forms.into_iter().map(Some).collect(),
            },
        };
        candidate.flags = flags.into_iter().collect();
        candidate.confidence = confidence;
        candidate.flag_notes = flag_notes;
        candidate.state = UnitState::Proposed;

        let report = self.catalogs[slot].gate(&candidate, self.project.glossary());
        let clean = report.findings.is_empty();
        let deliver = promotion == Promotion::Deliver && clean && !candidate.flags.has_hard();
        if deliver {
            candidate.state = UnitState::Finished;
        }
        self.catalogs[slot].doc.units_mut()[index] = candidate;
        self.catalogs[slot].dirty = true;
        self.record_review(
            slot,
            index,
            if deliver {
                ReviewStatus::Approved
            } else {
                ReviewStatus::MachineTranslated
            },
        )?;
        let catalog = &self.catalogs[slot];
        Ok(Applied {
            unit: self.view(catalog, &catalog.doc.units()[index]),
            translated: true,
            failure: None,
        })
    }

    /// Translate every string in `scope`, one at a time, reporting progress
    /// and stopping at the next string when `cancel` fires.
    ///
    /// # Errors
    ///
    /// The first backend error that is not specific to one string (server
    /// down, authentication); strings translated before it stay translated.
    pub fn translate(
        &mut self,
        scope: &TranslateScope,
        input: InputText,
        promotion: Promotion,
        backend: &dyn TranslationBackend,
        progress: &mut dyn FnMut(Progress),
        cancel: &CancelToken,
    ) -> Result<()> {
        let plan = self.plan(scope)?;
        let total = plan.len();
        progress(Progress::Started { total });
        let mut memo = Memo::new();
        let (mut translated, mut failed) = (0, 0);
        for (done, (path, id)) in plan.iter().enumerate() {
            if cancel.is_cancelled() {
                progress(Progress::Finished {
                    translated,
                    failed,
                    cancelled: true,
                });
                return Ok(());
            }
            let task = self.prepare(path, id, input)?;
            let (outcome, _) = memo.translate(backend, &task)?;
            let applied = self.apply(&task, outcome, promotion)?;
            if applied.translated {
                translated += 1;
            } else {
                failed += 1;
            }
            progress(Progress::Unit {
                catalog: path.display().to_string(),
                id: id.0.clone(),
                done: done + 1,
                total,
                translated: applied.translated,
                failure: applied.failure,
            });
        }
        progress(Progress::Finished {
            translated,
            failed,
            cancelled: false,
        });
        Ok(())
    }
}

fn empty_like(target: &Target) -> Target {
    match target {
        Target::Singular { .. } => Target::Singular { text: None },
        Target::Plural { forms } => Target::Plural {
            forms: vec![None; forms.len()],
        },
    }
}

/// The pivot-language text to translate from, when it is sound: complete,
/// with the same placeholders and markup as the original. A pivot text that
/// already lost a placeholder would pass its defect on to the translation.
fn sound_pivot_text(
    catalog: &crate::session::OpenCatalog,
    unit: &Unit,
    existing: &Target,
    glossary: Option<&Glossary>,
) -> Option<String> {
    if !has_complete_text(existing) {
        return None;
    }
    let mut probe = unit.clone();
    probe.target = existing.clone();
    probe.state = UnitState::Proposed;
    let report = catalog.gate(&probe, glossary);
    if report.flags.has_hard()
        || report
            .flags
            .contains(i18n_harness_core::Flag::MarkupTagMismatch)
    {
        return None;
    }
    match existing {
        Target::Singular { text } => text.clone(),
        Target::Plural { forms } => forms.first().cloned().flatten(),
    }
}
