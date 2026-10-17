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
    /// Counts as done unless it has an error; warnings stay visible but do
    /// not hold a string back. For unattended runs whose output is
    /// delivered as is.
    Deliver,
}

/// One text the model translates: a non-plural string, or one plural form
/// of a plural string with its count filled in.
#[derive(Debug, Clone)]
pub struct Segment {
    /// The unit as sent to the model: singular, its `source` the input text
    /// in the catalog's native placeholder syntax.
    pub request: Unit,
    /// For a plural form: the sample count written into the input and the
    /// native count placeholder (`%n`) to put back in the answer.
    pub count: Option<(u32, String)>,
}

/// Everything needed to translate one string, owned so the session need not
/// stay borrowed while the model works.
#[derive(Debug, Clone)]
pub struct TranslationTask {
    /// Catalog the string belongs to.
    pub catalog: PathBuf,
    /// The string's id.
    pub id: UnitId,
    /// What to send to the model: one segment, or one per plural form.
    pub segments: Vec<Segment>,
    /// Whether the string is plural (the answer is assembled from the
    /// segments in order).
    pub plural: bool,
    /// Locale id of the input text.
    pub input_locale: String,
    /// Target locale (project overrides applied).
    pub locale: Locale,
    /// Project glossary.
    pub glossary: Option<Glossary>,
}

/// Identity of a model request for de-duplication.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MemoKey {
    input_locale: String,
    locale: String,
    text: String,
}

/// Model answers already obtained in a run, so a text that occurs many
/// times is translated once.
#[derive(Debug, Default)]
pub struct Memo(HashMap<MemoKey, String>);

impl Memo {
    /// An empty memo.
    pub fn new() -> Self {
        Self::default()
    }

    /// The answer for `task`, asking `backend` only for segments not
    /// answered before. Returns the outcome and how many segments were
    /// answered from the memo.
    ///
    /// # Errors
    ///
    /// The backend's error.
    pub fn translate(
        &mut self,
        backend: &dyn TranslationBackend,
        task: &TranslationTask,
    ) -> Result<(TranslationOutcome, usize)> {
        let source = Locale::by_id(&task.input_locale)
            .ok_or_else(|| SessionError::UnknownLocale(task.input_locale.clone()))?;
        let mut forms = Vec::with_capacity(task.segments.len());
        let mut reused = 0;
        for segment in &task.segments {
            let key = MemoKey {
                input_locale: task.input_locale.clone(),
                locale: task.locale.id.to_owned(),
                text: segment.request.source.clone(),
            };
            let answer = match self.0.get(&key) {
                Some(answer) => {
                    reused += 1;
                    answer.clone()
                }
                None => match ask(backend, task, segment, source)? {
                    Ok(answer) => {
                        self.0.insert(key, answer.clone());
                        answer
                    }
                    Err(failure) => return Ok((failure, reused)),
                },
            };
            match restore_count(&answer, segment) {
                Some(form) => forms.push(form),
                None => {
                    return Ok((
                        TranslationOutcome::Failed {
                            reason: "the model changed the number in a plural form".into(),
                            retryable: false,
                            failure_kind: Default::default(),
                        },
                        reused,
                    ));
                }
            }
        }
        Ok((assemble(task, forms), reused))
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
    Ok(Memo::new().translate(backend, task)?.0)
}

/// One segment's answer, or the failure outcome for the whole string.
fn ask(
    backend: &dyn TranslationBackend,
    task: &TranslationTask,
    segment: &Segment,
    source: &Locale,
) -> Result<std::result::Result<String, TranslationOutcome>> {
    let batch = Batch::new(
        BatchKey::new(task.catalog.display().to_string(), 0),
        vec![segment.request.clone()],
    );
    let outcome = backend
        .translate_batch_from(&batch, source, &task.locale, task.glossary.as_ref())?
        .pop()
        .unwrap_or(TranslationOutcome::Failed {
            reason: "the model returned no answer".into(),
            retryable: true,
            failure_kind: Default::default(),
        });
    Ok(match outcome {
        TranslationOutcome::Translated {
            text: TranslatedText::Singular(text),
            ..
        } => Ok(text),
        TranslationOutcome::Translated {
            text: TranslatedText::Plural(mut forms),
            ..
        } if forms.len() == 1 => Ok(forms.remove(0)),
        TranslationOutcome::Translated { .. } => Err(TranslationOutcome::Failed {
            reason: "the model returned several forms for one text".into(),
            retryable: false,
            failure_kind: Default::default(),
        }),
        failed => Err(failed),
    })
}

/// Put the count placeholder back where the model wrote the sample count.
/// `None` when the number does not appear exactly as often as in the input.
fn restore_count(answer: &str, segment: &Segment) -> Option<String> {
    let Some((sample, token)) = &segment.count else {
        return Some(answer.to_owned());
    };
    let number = sample.to_string();
    let want = count_number(&segment.request.source, &number);
    if count_number(answer, &number) != want || want == 0 {
        return None;
    }
    Some(replace_number(answer, &number, token))
}

fn number_positions(text: &str, number: &str) -> Vec<usize> {
    text.match_indices(number)
        .filter(|(i, _)| {
            let before = text[..*i].chars().next_back();
            let after = text[i + number.len()..].chars().next();
            !before.is_some_and(|c| c.is_ascii_digit())
                && !after.is_some_and(|c| c.is_ascii_digit())
        })
        .map(|(i, _)| i)
        .collect()
}

fn count_number(text: &str, number: &str) -> usize {
    number_positions(text, number).len()
}

fn replace_number(text: &str, number: &str, token: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for i in number_positions(text, number) {
        out.push_str(&text[last..i]);
        out.push_str(token);
        last = i + number.len();
    }
    out.push_str(&text[last..]);
    out
}

fn assemble(task: &TranslationTask, forms: Vec<String>) -> TranslationOutcome {
    let text = if task.plural {
        TranslatedText::Plural(forms)
    } else {
        TranslatedText::Singular(forms.into_iter().next().unwrap_or_default())
    };
    TranslationOutcome::Translated {
        text,
        flags: Vec::new(),
        confidence: None,
        flag_notes: Default::default(),
    }
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

        let format = catalog.doc.format();
        let native = |icu: &str| {
            format
                .placeholders_from_icu(icu)
                .unwrap_or_else(|_| icu.to_owned())
        };

        // The text to translate from: the pivot-language text when it is
        // sound, else the original.
        let mut input_icu = unit.source.clone();
        let mut input_locale = "en".to_owned();
        if let (InputText::Auto, Some(pivot), Some(existing)) =
            (input, catalog.pivot, catalog.existing.get(id))
            && let Some(text) = sound_pivot_text(catalog, unit, existing, self.project().glossary())
        {
            input_icu = text;
            input_locale = pivot.id.to_owned();
        }
        let input_text = native(&input_icu);

        let mut base = unit.clone();
        base.target = Target::Singular { text: None };
        base.plural_arity = None;
        base.flags = FlagSet::new();
        base.flag_notes.clear();
        base.confidence = None;
        let segment = |text: String, count: Option<(u32, String)>| {
            let mut request = base.clone();
            request.source = text;
            Segment { request, count }
        };

        let plural = unit.plural_arity.is_some();
        let segments = if let Target::Plural { forms } = &unit.target {
            // One request per form, with a count that selects that form
            // written in place of the count placeholder.
            let token = native("{count}");
            let samples = plural_samples(&catalog.locale, forms.len());
            samples
                .into_iter()
                .map(|n| {
                    if input_text.contains(&token) {
                        segment(
                            input_text.replace(&token, &n.to_string()),
                            Some((n, token.clone())),
                        )
                    } else {
                        segment(input_text.clone(), None)
                    }
                })
                .collect()
        } else {
            vec![segment(input_text, None)]
        };

        Ok(TranslationTask {
            catalog: catalog.path.clone(),
            id: id.clone(),
            segments,
            plural,
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

        // The model answered in the catalog's native placeholder syntax.
        let format = self.catalogs[slot].doc.format();
        let to_icu = |text: String| format.placeholders_to_icu(&text);
        let target = match text {
            TranslatedText::Singular(s) => to_icu(s).map(|t| Target::Singular { text: Some(t) }),
            TranslatedText::Plural(forms) => forms
                .into_iter()
                .map(|f| to_icu(f).map(Some))
                .collect::<std::result::Result<Vec<_>, _>>()
                .map(|forms| Target::Plural { forms }),
        };
        let target = match target {
            Ok(target) => target,
            Err(error) => {
                let catalog = &self.catalogs[slot];
                return Ok(Applied {
                    unit: self.view(catalog, &catalog.doc.units()[index]),
                    translated: false,
                    failure: Some(format!(
                        "the model's placeholders cannot be stored: {error}"
                    )),
                });
            }
        };
        let mut candidate = self.catalogs[slot].doc.units()[index].clone();
        candidate.target = target;
        candidate.flags = flags.into_iter().collect();
        candidate.confidence = confidence;
        candidate.flag_notes = flag_notes;
        candidate.state = UnitState::Proposed;

        let report = self.catalogs[slot].gate(&candidate, self.project.glossary());
        // Delivery holds back only what is broken; a warning is a hint for a
        // reviewer, not a reason to ship the string unfinished.
        let deliver = promotion == Promotion::Deliver
            && !report.flags.has_hard()
            && !candidate.flags.has_hard();
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

/// A count selecting each of `forms` plural forms. The locale's integer
/// forms when they match the file's form count (Qt and gettext tables);
/// otherwise small counts in order, the last form taking a large one.
fn plural_samples(locale: &Locale, forms: usize) -> Vec<u32> {
    if locale.integer_plural_forms.len() == forms {
        return locale
            .integer_plural_forms
            .iter()
            .map(|(_, n)| *n)
            .collect();
    }
    (0..forms)
        .map(|i| {
            if i + 1 == forms {
                5
            } else {
                u32::try_from(i + 1).unwrap_or(1)
            }
        })
        .collect()
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
