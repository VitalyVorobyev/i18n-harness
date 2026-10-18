//! Strings shared between files of one language, the Show filters, and the
//! tunable length check.

use std::path::{Path, PathBuf};

use i18n_harness_session::{CheckSettings, Session, Show, UnitFilter};

fn ts(messages: &[(&str, &str, &str)]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE TS>\n<TS version=\"2.1\" language=\"sk_SK\">\n",
    );
    let mut contexts: Vec<&str> = messages.iter().map(|m| m.0).collect();
    contexts.dedup();
    for context in contexts {
        out.push_str(&format!("<context>\n    <name>{context}</name>\n"));
        for (_, source, translation) in messages.iter().filter(|m| m.0 == context) {
            let translation = if translation.is_empty() {
                "<translation type=\"unfinished\"></translation>".to_owned()
            } else {
                format!("<translation>{translation}</translation>")
            };
            out.push_str(&format!(
                "    <message>\n        <source>{source}</source>\n        {translation}\n    </message>\n"
            ));
        }
        out.push_str("</context>\n");
    }
    out.push_str("</TS>\n");
    out
}

/// A folder of Qt files for `sk_SK`, opened; paths in the order given.
fn open_files(files: &[(&str, String)]) -> (tempfile::TempDir, Session, Vec<PathBuf>) {
    let dir = tempfile::tempdir().expect("tempdir");
    for (name, text) in files {
        std::fs::write(dir.path().join(name), text).expect("write fixture");
    }
    let session = Session::open_folder(dir.path()).expect("open folder");
    let listed: Vec<PathBuf> = session
        .catalogs()
        .into_iter()
        .map(|c| PathBuf::from(c.path))
        .collect();
    let paths = files
        .iter()
        .map(|(name, _)| {
            listed
                .iter()
                .find(|p| p.file_name().is_some_and(|n| n == *name))
                .expect("catalog listed")
                .clone()
        })
        .collect();
    (dir, session, paths)
}

/// Three files: A and B share "Open" (same translation) and "Close"
/// (different translations); C has "Open" in another context, which is a
/// different string.
fn open() -> (tempfile::TempDir, Session, [PathBuf; 3]) {
    let (dir, session, paths) = open_files(&[
        (
            "A_sk_SK.ts",
            ts(&[
                ("Main", "Open", "Otvoriť"),
                ("Main", "Close", "Zavrieť"),
                ("Main", "Only in A", ""),
                (
                    "Main",
                    "Save changes",
                    "Uložiť všetky vykonané zmeny do súboru",
                ),
            ]),
        ),
        (
            "B_sk_SK.ts",
            ts(&[
                ("Main", "Open", "Otvoriť"),
                ("Main", "Close", "Zatvoriť"),
                ("Main", "Only in B", ""),
            ]),
        ),
        ("C_sk_SK.ts", ts(&[("Other", "Open", "Otvoriť")])),
    ]);
    let [a, b, c] = <[PathBuf; 3]>::try_from(paths).expect("three files");
    (dir, session, [a, b, c])
}

fn show(show: Show) -> UnitFilter {
    UnitFilter {
        show,
        ..UnitFilter::default()
    }
}

fn sources(session: &Session, path: Option<&Path>, filter: &UnitFilter) -> Vec<(String, String)> {
    session
        .rows(path, filter)
        .unwrap()
        .into_iter()
        .map(|r| (r.file, r.source))
        .collect()
}

#[test]
fn counts_tell_shared_inconsistent_and_only_here_strings_apart() {
    let (_dir, session, [a, b, c]) = open();
    let counts = |p: &Path| session.catalog_summary(p).unwrap().counts;
    let (a, b, c) = (counts(&a), counts(&b), counts(&c));
    assert_eq!((a.total, a.only_here, a.inconsistent), (4, 2, 1));
    assert_eq!((b.total, b.only_here, b.inconsistent), (3, 1, 1));
    // Same id text, other context: not the same string.
    assert_eq!((c.total, c.only_here, c.inconsistent), (1, 1, 0));
}

#[test]
fn rows_over_all_files_filter_by_how_strings_relate() {
    let (_dir, session, _) = open();
    assert_eq!(session.rows(None, &UnitFilter::default()).unwrap().len(), 8);
    assert_eq!(
        sources(&session, None, &show(Show::Inconsistent)),
        [
            ("A_sk_SK.ts".to_owned(), "Close".to_owned()),
            ("B_sk_SK.ts".to_owned(), "Close".to_owned()),
        ]
    );
    let shared = sources(&session, None, &show(Show::Shared));
    assert_eq!(shared.len(), 4, "{shared:?}");
    assert!(shared.iter().all(|(file, _)| file != "C_sk_SK.ts"));
}

#[test]
fn rows_carry_their_file_and_markers() {
    let (_dir, session, [a, _, _]) = open();
    let rows = session.rows(Some(&a), &UnitFilter::default()).unwrap();
    let close = rows.iter().find(|r| r.source == "Close").unwrap();
    assert!(close.shared && close.inconsistent);
    assert_eq!(close.file, "A_sk_SK.ts");
    assert_eq!(close.target, "Zavrieť");
    let only = rows.iter().find(|r| r.source == "Only in A").unwrap();
    assert!(!only.shared && !only.inconsistent);
}

#[test]
fn the_warnings_filter_lists_translations_with_a_warning() {
    let (_dir, session, [a, _, _]) = open();
    let warned = sources(&session, Some(&a), &show(Show::Warnings));
    assert_eq!(
        warned,
        [("A_sk_SK.ts".to_owned(), "Save changes".to_owned())]
    );
    assert_eq!(session.catalog_summary(&a).unwrap().counts.warnings, 1);
    assert!(sources(&session, Some(&a), &show(Show::Problems)).is_empty());
}

#[test]
fn length_check_settings_are_stored_and_applied() {
    let (dir, mut session, [a, _, _]) = open();
    let defaults = session.check_settings("sk_SK").unwrap();
    assert_eq!(defaults.length_min_chars, 8);

    // Longer originals only: "Save changes" (12 characters) is no longer checked.
    session
        .set_check_settings(
            "sk_SK",
            CheckSettings {
                length_ratio: defaults.length_ratio,
                length_min_chars: 20,
            },
        )
        .unwrap();
    assert!(sources(&session, Some(&a), &show(Show::Warnings)).is_empty());

    // A ratio that allows the expansion has the same effect.
    session
        .set_check_settings(
            "sk_SK",
            CheckSettings {
                length_ratio: 4.0,
                length_min_chars: 8,
            },
        )
        .unwrap();
    assert!(sources(&session, Some(&a), &show(Show::Warnings)).is_empty());

    // Stored in the settings file, so a restart keeps them.
    let manifest = std::fs::read_to_string(dir.path().join("i18n-harness.toml")).unwrap();
    assert!(manifest.contains("length_warn_ratio = 4"), "{manifest}");
    let reopened = Session::open_folder(dir.path()).unwrap();
    let settings = reopened.check_settings("sk_SK").unwrap();
    assert_eq!(settings.length_min_chars, 8);
    assert!((settings.length_ratio - 4.0).abs() < f32::EPSILON);
    assert_eq!(reopened.catalog_summary(&a).unwrap().counts.warnings, 0);
}

#[test]
fn a_ratio_that_is_not_positive_is_refused() {
    let (_dir, mut session, _) = open();
    let bad = CheckSettings {
        length_ratio: 0.0,
        length_min_chars: 8,
    };
    assert!(session.set_check_settings("sk_SK", bad).is_err());
}

#[test]
fn glossary_usage_counts_strings_and_translations_that_break_a_term() {
    use i18n_harness_session::GlossaryEntry;
    let (_dir, mut session, _) = open();
    let entry = |term: &str, keep: bool, translation: Option<&str>| GlossaryEntry {
        term: term.to_owned(),
        keep,
        translation: translation.map(str::to_owned),
        note: None,
    };
    session
        .set_glossary(
            "sk_SK",
            vec![
                entry("Open", false, Some("Otvoriť")),
                entry("Close", true, None),
                entry("Print", true, None),
            ],
        )
        .unwrap();
    let usage = session.glossary_usage("sk_SK");
    let of = |term: &str| {
        let u = usage.iter().find(|u| u.term == term).unwrap();
        (u.strings, u.breaking)
    };
    // "Open" in all three files, translated as required everywhere.
    assert_eq!(of("Open"), (3, 0));
    // "Close" must stay unchanged; both translations change it.
    assert_eq!(of("Close"), (2, 2));
    assert_eq!(of("Print"), (0, 0));
    assert!(
        session
            .glossary_usage("de_DE")
            .iter()
            .all(|u| u.strings == 0)
    );
}

/// A translates Open and Close; B agrees on Open only; C has both to do.
fn open_to_fill() -> (tempfile::TempDir, Session, Vec<PathBuf>) {
    open_files(&[
        (
            "A_sk_SK.ts",
            ts(&[("Main", "Open", "Otvoriť"), ("Main", "Close", "Zavrieť")]),
        ),
        (
            "B_sk_SK.ts",
            ts(&[("Main", "Open", "Otvoriť"), ("Main", "Close", "Zatvoriť")]),
        ),
        (
            "C_sk_SK.ts",
            ts(&[
                ("Main", "Open", ""),
                ("Main", "Close", ""),
                ("Main", "New", ""),
            ]),
        ),
    ])
}

#[test]
fn filling_copies_what_other_files_agree_on_and_leaves_conflicts_alone() {
    let (_dir, mut session, paths) = open_to_fill();
    let c = &paths[2];
    // A preview changes nothing.
    let preview = session.fill_from_other_files(&[], false).unwrap();
    assert_eq!((preview.filled, preview.conflicting), (1, 1));
    assert_eq!(session.catalog_summary(c).unwrap().counts.to_do, 3);

    let report = session
        .fill_from_other_files(std::slice::from_ref(c), true)
        .unwrap();
    assert_eq!((report.filled, report.conflicting), (1, 1));
    let rows = session.rows(Some(c), &UnitFilter::default()).unwrap();
    let open = rows.iter().find(|r| r.source == "Open").unwrap();
    // Done in the other files, so done here too.
    assert_eq!(open.target, "Otvoriť");
    assert_eq!(open.status, i18n_harness_session::UnitStatus::Done);
    let close = rows.iter().find(|r| r.source == "Close").unwrap();
    assert_eq!(close.target, "");
    assert!(close.inconsistent);
    assert!(session.catalog_summary(c).unwrap().dirty);
}

#[test]
fn siblings_show_each_files_translation() {
    let (_dir, session, paths) = open_to_fill();
    let id = session
        .rows(Some(&paths[2]), &UnitFilter::default())
        .unwrap()[1]
        .id
        .clone();
    let siblings = session.siblings(&paths[2], &id).unwrap();
    let mut seen: Vec<(String, Vec<String>)> =
        siblings.into_iter().map(|s| (s.file, s.target)).collect();
    seen.sort();
    assert_eq!(
        seen,
        [
            ("A_sk_SK.ts".to_owned(), vec!["Zavrieť".to_owned()]),
            ("B_sk_SK.ts".to_owned(), vec!["Zatvoriť".to_owned()]),
        ]
    );
}

#[test]
fn one_translation_can_be_used_everywhere() {
    let (_dir, mut session, paths) = open_to_fill();
    let close = |s: &Session, p: &Path| {
        s.rows(Some(p), &UnitFilter::default())
            .unwrap()
            .into_iter()
            .find(|r| r.source == "Close")
            .unwrap()
    };
    let id = close(&session, &paths[0]).id;
    assert_eq!(session.use_everywhere(&paths[0], &id).unwrap(), 2);
    for path in &paths {
        let row = close(&session, path);
        assert_eq!(row.target, "Zavrieť");
        assert!(!row.inconsistent);
    }
    assert!(session.catalog_summary(&paths[1]).unwrap().dirty);
    // Nothing to copy from a string without a translation.
    let new = session
        .rows(Some(&paths[2]), &UnitFilter::default())
        .unwrap()
        .into_iter()
        .find(|r| r.source == "New")
        .unwrap();
    assert!(session.use_everywhere(&paths[2], &new.id).is_err());
}

#[test]
fn shared_strings_list_each_string_in_several_files_with_its_translations() {
    let (_dir, session, _) = open_to_fill();
    let shared = session.shared_strings();
    let of = |source: &str| shared.iter().find(|s| s.source == source).unwrap();
    // A and B agree on Open; C has it to do.
    let opened = of("Open");
    assert_eq!(opened.members.len(), 3);
    assert_eq!((opened.translations, opened.missing), (1, 1));
    assert_eq!(opened.members[0].target, ["Otvoriť"]);
    assert_eq!(opened.members[2].target, [""]);
    assert!(!opened.existing_differs);
    // A string in one file only is not listed.
    assert!(shared.iter().all(|s| s.source != "New"));

    let (_dir, session, _) = open();
    let shared = session.shared_strings();
    let close = shared.iter().find(|s| s.source == "Close").unwrap();
    assert_eq!((close.translations, close.missing), (2, 0));
    // "Open" of C has another context: A and B only.
    let opened = shared.iter().find(|s| s.source == "Open").unwrap();
    assert_eq!(opened.members.len(), 2);
}

#[test]
fn strings_are_filtered_by_glossary_term_and_by_broken_terms() {
    use i18n_harness_session::GlossaryEntry;
    let (_dir, mut session, [a, _, _]) = open();
    session
        .set_glossary(
            "sk_SK",
            vec![
                GlossaryEntry {
                    term: "Open".to_owned(),
                    keep: false,
                    translation: Some("Otvoriť".to_owned()),
                    note: None,
                },
                GlossaryEntry {
                    term: "Close".to_owned(),
                    keep: true,
                    translation: None,
                    note: None,
                },
            ],
        )
        .unwrap();
    let term = |t: &str| UnitFilter {
        term: Some(t.to_owned()),
        ..UnitFilter::default()
    };
    assert_eq!(
        sources(&session, None, &term("Close")),
        [
            ("A_sk_SK.ts".to_owned(), "Close".to_owned()),
            ("B_sk_SK.ts".to_owned(), "Close".to_owned()),
        ]
    );
    assert_eq!(sources(&session, None, &show(Show::Glossary)).len(), 5);
    // Both translations of Close change a term that must stay.
    assert_eq!(
        sources(&session, None, &show(Show::GlossaryBroken)).len(),
        2
    );

    let rows = session.rows(Some(&a), &UnitFilter::default()).unwrap();
    let close = rows.iter().find(|r| r.source == "Close").unwrap();
    assert_eq!((close.terms, close.glossary_broken), (1, true));
    let counts = session.catalog_summary(&a).unwrap().counts;
    assert_eq!((counts.glossary, counts.glossary_broken), (2, 1));

    let unit = session.unit(&a, &close.id).unwrap();
    assert_eq!(unit.terms.len(), 1);
    assert!(unit.terms[0].keep && unit.terms[0].broken);
    let open = rows.iter().find(|r| r.source == "Open").unwrap();
    let unit = session.unit(&a, &open.id).unwrap();
    assert_eq!(unit.terms[0].translation.as_deref(), Some("Otvoriť"));
    assert!(!unit.terms[0].broken);
}
