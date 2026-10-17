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

/// Three files: A and B share "Open" (same translation) and "Close"
/// (different translations); C has "Open" in another context, which is a
/// different string.
fn open() -> (tempfile::TempDir, Session, [PathBuf; 3]) {
    let dir = tempfile::tempdir().expect("tempdir");
    let files = [
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
    ];
    for (name, text) in &files {
        std::fs::write(dir.path().join(name), text).expect("write fixture");
    }
    let session = Session::open_folder(dir.path()).expect("open folder");
    let path = |name: &str| {
        session
            .catalogs()
            .into_iter()
            .map(|c| PathBuf::from(c.path))
            .find(|p| p.file_name().is_some_and(|n| n == name))
            .expect("catalog listed")
    };
    let paths = [path("A_sk_SK.ts"), path("B_sk_SK.ts"), path("C_sk_SK.ts")];
    (dir, session, paths)
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
