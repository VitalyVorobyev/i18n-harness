//! Session behaviour on a copy of the Slovak pivot fixture: open a bare
//! folder, statuses, pivot input, translation, save guard.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use i18n_harness_backend::{ManualBackend, ManualResponse, PromptContext};
use i18n_harness_session::{
    CancelToken, InputText, IssueSeverity, Progress, Promotion, Session, TranslateScope,
    UnitFilter, UnitStatus,
};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/qt/sk_SK_pivot.ts")
}

/// A fresh folder holding one copy of the fixture, opened as a session.
fn open() -> (tempfile::TempDir, Session, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("App_sk_SK.ts");
    std::fs::copy(fixture(), &path).expect("copy fixture");
    let session = Session::open_folder(dir.path()).expect("open folder");
    let path = std::fs::canonicalize(&path).unwrap_or(path);
    let path = session
        .catalogs()
        .into_iter()
        .map(|c| PathBuf::from(c.path))
        .find(|p| p.file_name() == path.file_name())
        .expect("catalog listed");
    (dir, session, path)
}

fn ids_with(session: &Session, path: &Path, status: UnitStatus) -> Vec<String> {
    let filter = UnitFilter {
        statuses: vec![status],
        ..UnitFilter::default()
    };
    session
        .units(path, &filter, 0, usize::MAX)
        .unwrap()
        .units
        .into_iter()
        .map(|u| u.id)
        .collect()
}

#[test]
fn opening_a_bare_folder_creates_the_settings_file_and_detects_the_catalog() {
    let (dir, session, _) = open();
    assert!(dir.path().join("i18n-harness.toml").is_file());
    let catalogs = session.catalogs();
    assert_eq!(catalogs.len(), 1);
    let c = &catalogs[0];
    assert_eq!(c.format, "qt-ts");
    assert_eq!(c.format_name, "Qt Linguist");
    assert_eq!(c.locale, "sk_SK");
    assert_eq!(c.language, "Slovak");
    assert_eq!(c.counts.total, 6);
    // Without a pivot, the German texts count as finished translations.
    // To do: the empty string, the empty unfinished one, and the plural whose
    // second and third forms are empty.
    assert_eq!(c.counts.to_do, 3);
}

#[test]
fn a_pivot_language_turns_existing_texts_into_work_to_do() {
    let (_dir, mut session, path) = open();
    session.set_pivot(&path, Some("de_DE")).expect("set pivot");
    let c = session.catalog_summary(&path).unwrap();
    assert_eq!(c.pivot.as_deref(), Some("de_DE"));
    assert_eq!(c.pivot_language.as_deref(), Some("German"));
    // The unfinished, empty one is "to do" either way; the finished German
    // ones now are too.
    assert_eq!(c.counts.to_do, 6);
    // The setting survives a restart.
    let reopened = Session::open_folder(session.root()).unwrap();
    assert_eq!(reopened.catalog_summary(&path).unwrap().counts.to_do, 6);
}

#[test]
fn views_show_native_placeholders_existing_text_and_plural_slots() {
    let (_dir, session, path) = open();
    let page = session
        .units(&path, &UnitFilter::default(), 0, usize::MAX)
        .unwrap();
    let markup = page
        .units
        .iter()
        .find(|u| u.source.contains("Ignore"))
        .unwrap();
    assert_eq!(markup.source, "<b>Ignore</b> defects under %1 mm");
    assert_eq!(
        markup.existing.as_deref(),
        Some(&["<b>Ignoriere</b> Defekte unter %1 mm".to_owned()][..])
    );
    let plural = page.units.iter().find(|u| u.source.contains("%n")).unwrap();
    let categories: Vec<_> = plural
        .plural_slots
        .iter()
        .map(|s| s.category.clone().unwrap())
        .collect();
    assert_eq!(categories, ["one", "few", "other"]);
    assert_eq!(plural.context.group.as_deref(), Some("Import"));
}

#[test]
fn untouched_pivot_text_is_shown_as_input_not_as_translation() {
    let (_dir, mut session, path) = open();
    session.set_pivot(&path, Some("de_DE")).unwrap();
    let unit = session
        .units(&path, &UnitFilter::default(), 0, usize::MAX)
        .unwrap()
        .units
        .into_iter()
        .find(|u| u.source == "Filter Size")
        .unwrap();
    assert_eq!(
        unit.existing.as_deref(),
        Some(&["Filtergröße".to_owned()][..])
    );
    assert_eq!(unit.target, [""]);
}

#[test]
fn search_filters_by_text_and_context() {
    let (_dir, session, path) = open();
    let filter = |q: &str| UnitFilter {
        search: Some(q.into()),
        ..UnitFilter::default()
    };
    assert_eq!(
        session
            .units(&path, &filter("paramsmanager"), 0, 10)
            .unwrap()
            .matching,
        1
    );
    assert_eq!(
        session
            .units(&path, &filter("Import"), 0, 10)
            .unwrap()
            .matching,
        2
    );
    assert_eq!(
        session
            .units(&path, &filter("Filtergröße"), 0, 10)
            .unwrap()
            .matching,
        1
    );
}

#[test]
fn pivot_input_is_the_existing_text_and_falls_back_to_the_source() {
    let (_dir, mut session, path) = open();
    session.set_pivot(&path, Some("de_DE")).unwrap();
    let plan = session.plan(&TranslateScope::default()).unwrap();
    assert_eq!(plan.len(), 6);

    let by_source = |session: &Session, needle: &str| {
        plan.iter()
            .map(|(p, id)| session.prepare(p, id, InputText::Auto).unwrap())
            .find(|t| t.id.0.contains(needle))
            .unwrap()
    };
    // Sound German: translate from it.
    let task = by_source(&session, "Filter Size");
    assert_eq!(task.segments[0].request.source, "Filtergröße");
    assert_eq!(task.input_locale, "de_DE");
    // No German: translate from English.
    let task = by_source(&session, "pix");
    assert_eq!(task.segments[0].request.source, "pix");
    assert_eq!(task.input_locale, "en");
    // German plural with empty forms is incomplete: English.
    let task = by_source(&session, "file(s)");
    assert_eq!(task.input_locale, "en");
    // `InputText::Source` ignores the pivot.
    let (p, id) = &plan[0];
    assert_eq!(
        session
            .prepare(p, id, InputText::Source)
            .unwrap()
            .input_locale,
        "en"
    );
}

/// A backend that "translates" by tagging the input, so tests can see which
/// text it received. It only ever sees single forms: the session expands
/// plural strings itself.
fn tagging_backend(
    seen: &Mutex<Vec<String>>,
) -> impl i18n_harness_backend::TranslationBackend + '_ {
    ManualBackend::new(move |ctx: &PromptContext<'_>| {
        assert!(
            ctx.unit.plural_arity.is_none(),
            "plural reached the backend"
        );
        seen.lock().unwrap().push(ctx.unit.source.clone());
        ManualResponse::Singular(format!("SK:{}", ctx.unit.source))
    })
}

#[test]
fn translate_marks_results_for_review_and_reports_progress() {
    let (_dir, mut session, path) = open();
    session.set_pivot(&path, Some("de_DE")).unwrap();
    let seen = Mutex::new(Vec::new());
    let backend = tagging_backend(&seen);
    let mut events = Vec::new();
    session
        .translate(
            &TranslateScope::default(),
            InputText::Auto,
            Promotion::Review,
            &backend,
            &mut |p| events.push(p),
            &CancelToken::new(),
        )
        .unwrap();

    assert!(matches!(
        events.first(),
        Some(Progress::Started { total: 6 })
    ));
    assert!(matches!(
        events.last(),
        Some(Progress::Finished {
            translated: 6,
            failed: 0,
            cancelled: false
        })
    ));
    assert!(seen.lock().unwrap().contains(&"Filtergröße".to_owned()));
    let c = session.catalog_summary(&path).unwrap();
    assert_eq!(c.counts.check, 6);
    assert!(c.dirty);
    let unit = session
        .units(&path, &UnitFilter::default(), 0, usize::MAX)
        .unwrap()
        .units
        .into_iter()
        .find(|u| u.source == "Filter Size")
        .unwrap();
    assert_eq!(unit.target, ["SK:Filtergröße"]);
}

#[test]
fn cancel_stops_before_the_next_string() {
    let (_dir, mut session, _path) = open();
    let seen = Mutex::new(Vec::new());
    let backend = tagging_backend(&seen);
    let cancel = CancelToken::new();
    let mut events = Vec::new();
    session
        .translate(
            &TranslateScope::default(),
            InputText::Auto,
            Promotion::Review,
            &backend,
            &mut |p| {
                if matches!(p, Progress::Unit { .. }) {
                    cancel.cancel();
                }
                events.push(p);
            },
            &cancel,
        )
        .unwrap();
    assert_eq!(seen.lock().unwrap().len(), 1);
    assert!(matches!(
        events.last(),
        Some(Progress::Finished {
            cancelled: true,
            ..
        })
    ));
}

#[test]
fn deliver_marks_clean_results_done_and_save_writes_them() {
    let (_dir, mut session, path) = open();
    session.set_pivot(&path, Some("de_DE")).unwrap();
    let seen = Mutex::new(Vec::new());
    let backend = tagging_backend(&seen);
    session
        .translate(
            &TranslateScope::default(),
            InputText::Auto,
            Promotion::Deliver,
            &backend,
            &mut |_| {},
            &CancelToken::new(),
        )
        .unwrap();
    let report = session.save(None).unwrap();
    assert_eq!(report.written.len(), 1);
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(
        written.contains("<translation>SK:Filtergröße</translation>"),
        "{written}"
    );
    assert_eq!(
        written
            .matches("<numerusform>SK:%n file(s) successfully imported.</numerusform>")
            .count(),
        3,
        "{written}"
    );
    // Everything outside the translations is byte-identical.
    assert!(written.contains("<location filename=\"../src/settings.cpp\" line=\"+29\"/>"));
    assert!(!session.catalog_summary(&path).unwrap().dirty);
}

#[test]
fn a_translation_that_breaks_a_placeholder_is_never_saved() {
    let (_dir, mut session, path) = open();
    let id = session
        .units(&path, &UnitFilter::default(), 0, usize::MAX)
        .unwrap()
        .units
        .into_iter()
        .find(|u| u.source.contains("%1 mm"))
        .unwrap()
        .id;
    let view = session
        .set_translation(
            &path,
            &id,
            vec!["<b>Ignorovať</b> chyby pod mm".into()],
            true,
        )
        .unwrap();
    assert!(
        view.issues
            .iter()
            .any(|i| i.severity == IssueSeverity::Error && i.message.contains("%1")),
        "{:?}",
        view.issues
    );
    let report = session.save(None).unwrap();
    assert_eq!(report.held_back, 1);
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(!written.contains("Ignorovať"));
    assert!(
        written.contains("Ignoriere"),
        "the file keeps its previous text"
    );
    // The broken edit is still there to fix.
    assert!(session.catalog_summary(&path).unwrap().dirty);
}

#[test]
fn done_survives_a_restart_through_the_review_store() {
    let (_dir, mut session, path) = open();
    session.set_pivot(&path, Some("de_DE")).unwrap();
    let id = ids_with(&session, &path, UnitStatus::ToDo)[0].clone();
    session
        .set_translation(&path, &id, vec!["Veľkosť filtra".into()], true)
        .unwrap();
    session.save(None).unwrap();
    let reopened = Session::open_folder(session.root()).unwrap();
    assert!(ids_with(&reopened, &path, UnitStatus::Done).contains(&id));
}

#[test]
fn glossary_edits_create_the_file_and_reach_the_model() {
    let (dir, mut session, path) = open();
    assert!(session.glossary("sk_SK").is_empty());
    session
        .set_glossary(
            "sk_SK",
            vec![
                i18n_harness_session::GlossaryEntry {
                    term: "ParamsManager".into(),
                    keep: true,
                    translation: None,
                    note: Some("class name".into()),
                },
                i18n_harness_session::GlossaryEntry {
                    term: "Filter".into(),
                    keep: false,
                    translation: Some("Filter".into()),
                    note: None,
                },
            ],
        )
        .unwrap();
    assert!(dir.path().join("glossary.toml").is_file());
    let entries = session.glossary("sk_SK");
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|e| e.term == "ParamsManager" && e.keep));

    // The model sees the glossary.
    let seen = Mutex::new(Vec::new());
    let backend = ManualBackend::new(|ctx: &PromptContext<'_>| {
        let dnt: Vec<String> = ctx
            .glossary
            .map(|g| g.do_not_translate().map(str::to_owned).collect())
            .unwrap_or_default();
        seen.lock().unwrap().push(dnt);
        ManualResponse::Skip
    });
    let plan = session.plan(&TranslateScope::default()).unwrap();
    let task = session.prepare(&path, &plan[0].1, InputText::Auto).unwrap();
    i18n_harness_session::translate_task(&backend, &task).unwrap();
    assert_eq!(seen.lock().unwrap()[0], ["ParamsManager"]);

    // Removing a term from the list removes it from the file.
    session.set_glossary("sk_SK", Vec::new()).unwrap();
    assert!(session.glossary("sk_SK").is_empty());
}

#[test]
fn plural_forms_are_translated_with_their_sample_counts() {
    let (_dir, mut session, path) = open();
    let plural_id = session
        .units(&path, &UnitFilter::default(), 0, usize::MAX)
        .unwrap()
        .units
        .into_iter()
        .find(|u| !u.plural_slots.is_empty())
        .unwrap()
        .id;
    let task = session
        .prepare(
            &path,
            &i18n_harness_core::UnitId::from(plural_id.as_str()),
            InputText::Auto,
        )
        .unwrap();
    let inputs: Vec<&str> = task
        .segments
        .iter()
        .map(|s| s.request.source.as_str())
        .collect();
    assert_eq!(
        inputs,
        [
            "1 file(s) successfully imported.",
            "2 file(s) successfully imported.",
            "0 file(s) successfully imported."
        ]
    );

    // A Slovak "model" that declines by the number it sees.
    let backend = ManualBackend::new(|ctx: &PromptContext<'_>| {
        let n = ctx.unit.source.split(' ').next().unwrap().to_owned();
        let noun = match n.as_str() {
            "1" => "súbor bol úspešne importovaný",
            "2" => "súbory boli úspešne importované",
            _ => "súborov bolo úspešne importovaných",
        };
        ManualResponse::Singular(format!("{n} {noun}."))
    });
    let outcome = i18n_harness_session::translate_task(&backend, &task).unwrap();
    let applied = session.apply(&task, outcome, Promotion::Review).unwrap();
    assert!(applied.translated, "{:?}", applied.failure);
    assert_eq!(
        applied.unit.target,
        [
            "%n súbor bol úspešne importovaný.",
            "%n súbory boli úspešne importované.",
            "%n súborov bolo úspešne importovaných."
        ]
    );
    assert!(
        applied
            .unit
            .issues
            .iter()
            .all(|i| i.severity != IssueSeverity::Error),
        "{:?}",
        applied.unit.issues
    );

    // A model that drops the number fails the string instead of guessing.
    let lossy = ManualBackend::new(|_: &PromptContext<'_>| {
        ManualResponse::Singular("súbory importované".into())
    });
    let outcome = i18n_harness_session::translate_task(&lossy, &task).unwrap();
    assert!(
        !session
            .apply(&task, outcome, Promotion::Review)
            .unwrap()
            .translated
    );
}

#[test]
fn a_translation_identical_to_the_pivot_text_is_shown_once_recorded() {
    let (_dir, mut session, path) = open();
    session.set_pivot(&path, Some("de_DE")).unwrap();
    let plan = session.plan(&TranslateScope::default()).unwrap();
    let (p, id) = plan
        .iter()
        .find(|(_, id)| id.0.contains("Filter Size"))
        .unwrap();
    let task = session.prepare(p, id, InputText::Auto).unwrap();
    // A "model" that returns its input unchanged.
    let echo = ManualBackend::new(|ctx: &PromptContext<'_>| {
        ManualResponse::Singular(ctx.unit.source.clone())
    });
    let outcome = i18n_harness_session::translate_task(&echo, &task).unwrap();
    let applied = session.apply(&task, outcome, Promotion::Review).unwrap();
    assert_eq!(applied.unit.target, ["Filtergröße"]);
    assert_eq!(applied.unit.status, UnitStatus::Check);
}

#[test]
fn a_saved_translation_awaiting_review_is_unfinished_in_a_pivot_file() {
    let (_dir, mut session, path) = open();
    session.set_pivot(&path, Some("de_DE")).unwrap();
    let plan = session.plan(&TranslateScope::default()).unwrap();
    let (p, id) = plan
        .iter()
        .find(|(_, id)| id.0.contains("Filter Size"))
        .unwrap();
    let task = session.prepare(p, id, InputText::Auto).unwrap();
    let backend = ManualBackend::new(|_: &PromptContext<'_>| {
        ManualResponse::Singular("Veľkosť filtra".into())
    });
    let outcome = i18n_harness_session::translate_task(&backend, &task).unwrap();
    session.apply(&task, outcome, Promotion::Review).unwrap();
    session.save(None).unwrap();

    let written = std::fs::read_to_string(&path).unwrap();
    assert!(
        written.contains(r#"<translation type="unfinished">Veľkosť filtra</translation>"#),
        "{written}"
    );
}

#[test]
fn gettext_plural_forms_translate_from_msgid_plural_with_their_count() {
    let dir = tempfile::tempdir().unwrap();
    let po = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/po/plurals_polish.po");
    let text = std::fs::read_to_string(po)
        .unwrap()
        .replace("Language: pl_PL", "Language: sk_SK")
        .replace(
            "msgstr[0] \"%d nowa wiadomość\"\nmsgstr[1] \"%d nowe wiadomości\"\nmsgstr[2] \"%d nowych wiadomości\"",
            "msgstr[0] \"\"\nmsgstr[1] \"\"\nmsgstr[2] \"\"",
        );
    std::fs::write(dir.path().join("sk.po"), text).unwrap();
    let mut session = Session::open_folder(dir.path()).unwrap();
    let path = PathBuf::from(&session.catalogs()[0].path);
    let plan = session.plan(&TranslateScope::default()).unwrap();
    let (p, id) = plan
        .iter()
        .find(|(_, id)| id.0.contains("new message"))
        .unwrap();
    let task = session.prepare(p, id, InputText::Auto).unwrap();
    let inputs: Vec<(&str, Option<u32>)> = task
        .segments
        .iter()
        .map(|s| (s.request.source.as_str(), s.count.as_ref().map(|(n, _)| *n)))
        .collect();
    assert_eq!(
        inputs,
        [
            ("1 new message", Some(1)),
            ("2 new messages", Some(2)),
            ("0 new messages", Some(0)),
        ]
    );

    let backend = ManualBackend::new(|ctx: &PromptContext<'_>| {
        let n: String = ctx
            .unit
            .source
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        ManualResponse::Singular(match n.as_str() {
            "1" => "1 nová správa".into(),
            "2" => "2 nové správy".into(),
            _ => format!("{n} nových správ"),
        })
    });
    let outcome = i18n_harness_session::translate_task(&backend, &task).unwrap();
    let applied = session.apply(&task, outcome, Promotion::Review).unwrap();
    assert!(applied.translated, "{:?}", applied.failure);
    session.save(None).unwrap();
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(
        written.contains(
            "msgstr[0] \"%d nová správa\"\nmsgstr[1] \"%d nové správy\"\nmsgstr[2] \"%d nových správ\""
        ),
        "{written}"
    );
}
