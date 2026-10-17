//! Fixture table for every gate rule.
//!
//! Each row pairs a constructed [`Unit`] with a locale and the set of flag
//! kinds the gate must produce. A single driver test walks the table and
//! diffs `actual` vs `expected`, so adding a new rule means appending one
//! `(positive, negative)` pair here — no boilerplate.
//!
//! Naming convention: `<rule>_<positive|negative>_<context>`. Positive =
//! the rule's flag is expected; negative = the rule's flag must NOT fire.

use i18n_harness_core::{Flag, FlagSet, Placeholder, Target, Unit, UnitId, UnitState};
use i18n_harness_gate::validate;
use i18n_harness_locales::Locale;

/// A test fixture row: a unit, the locale to validate against, and the
/// flags the gate must produce. Soft flags that must NOT fire are listed
/// in `forbidden` so an over-eager soft check still trips the test.
struct Fixture {
    name: &'static str,
    unit: Unit,
    locale: &'static Locale,
    expected: &'static [Flag],
    forbidden: &'static [Flag],
}

fn de_de() -> &'static Locale {
    Locale::by_id("de_DE").expect("de_DE present")
}

fn es_es() -> &'static Locale {
    Locale::by_id("es_ES").expect("es_ES present")
}

fn zh_hans() -> &'static Locale {
    Locale::by_id("zh_Hans").expect("zh_Hans present")
}

fn make_singular(id: &str, source: &str, target: Option<&str>, state: UnitState) -> Unit {
    let mut u = Unit::untranslated_singular(UnitId::from(id), source);
    u.target = Target::Singular {
        text: target.map(str::to_owned),
    };
    u.state = state;
    u
}

fn make_plural(id: &str, source: &str, forms: Vec<Option<&str>>, state: UnitState) -> Unit {
    let mut u = Unit::untranslated_singular(UnitId::from(id), source);
    u.plural_arity = Some(2);
    u.target = Target::Plural {
        forms: forms.into_iter().map(|f| f.map(str::to_owned)).collect(),
    };
    u.placeholders.push(Placeholder::plural_count(0));
    u.state = state;
    u
}

fn run_fixtures(table: &[Fixture]) {
    let mut failures: Vec<String> = Vec::new();
    for fx in table {
        let report = validate(&fx.unit, fx.locale, None);
        let actual: FlagSet = report.flags.clone();
        let expected: FlagSet = fx.expected.iter().copied().collect();
        let mut row_errs: Vec<String> = Vec::new();
        for f in fx.expected {
            if !actual.contains(*f) {
                row_errs.push(format!("expected flag {f:?} missing"));
            }
        }
        for f in fx.forbidden {
            if actual.contains(*f) {
                row_errs.push(format!("forbidden flag {f:?} fired"));
            }
        }
        // Catch over-firing of expected flags (a check that produces a flag
        // we did not list).
        for f in &actual {
            if !fx.expected.contains(&f) && !fx.forbidden.contains(&f) {
                // Allow undeclared flags only if they are not hard.
                if matches!(f.severity(), i18n_harness_core::FlagSeverity::Hard) {
                    row_errs.push(format!(
                        "unexpected hard flag {f:?} fired (declare it in `expected` or `forbidden`)"
                    ));
                }
            }
        }
        if !row_errs.is_empty() {
            failures.push(format!(
                "[{}] actual={actual:?} expected={expected:?}: {}",
                fx.name,
                row_errs.join("; ")
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "gate fixture table failures:\n{}",
        failures.join("\n")
    );
}

// ── Tests ────────────────────────────────────────────────────────────────

#[test]
fn gate_rule_table_drives_every_check() {
    let table: &[Fixture] = &[
        Fixture {
            name: "placeholder-multiset/positive/dropped-arg",
            unit: make_singular(
                "msg::open",
                "Open {0} from {1}",
                Some("Datei öffnen"),
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[Flag::PlaceholderMismatch],
            forbidden: &[],
        },
        Fixture {
            name: "placeholder-multiset/positive/duplicate-dropped",
            unit: make_singular(
                "msg::dup",
                "Found {0} and {0}",
                Some("{0} gefunden"),
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[Flag::PlaceholderMismatch],
            forbidden: &[],
        },
        Fixture {
            name: "placeholder-multiset/positive/extra-token",
            unit: make_singular(
                "msg::extra",
                "Open file",
                Some("Datei {0} öffnen"),
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[Flag::PlaceholderMismatch],
            forbidden: &[],
        },
        Fixture {
            name: "placeholder-multiset/negative/identical",
            unit: make_singular(
                "msg::ok",
                "Open {0} from {1}",
                Some("{0} aus {1} öffnen"),
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[],
            forbidden: &[Flag::PlaceholderMismatch],
        },
        Fixture {
            name: "plural-arity/positive/too-few-forms",
            unit: make_plural(
                "msg::msgs",
                "{count} unread message(s)",
                vec![Some("{count} ungelesene Nachricht")],
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[Flag::PluralArityMismatch],
            forbidden: &[],
        },
        Fixture {
            name: "plural-arity/positive/singular-target-on-plural-unit",
            unit: {
                // The placeholder multiset is intentionally clean here so
                // the test isolates the arity-vs-variant check.
                let mut u = make_singular(
                    "msg::wrong-variant",
                    "{count} unread message(s)",
                    Some("{count} Nachricht"),
                    UnitState::Proposed,
                );
                u.plural_arity = Some(2);
                u.placeholders.push(Placeholder::plural_count(0));
                u
            },
            locale: de_de(),
            expected: &[Flag::PluralArityMismatch],
            forbidden: &[Flag::PlaceholderMismatch],
        },
        Fixture {
            name: "plural-arity/negative/matches",
            unit: make_plural(
                "msg::msgs-ok",
                "{count} unread message(s)",
                vec![
                    Some("{count} ungelesene Nachricht"),
                    Some("{count} ungelesene Nachrichten"),
                ],
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[],
            forbidden: &[Flag::PluralArityMismatch],
        },
        Fixture {
            name: "icu-parse/positive/unbalanced-brace",
            unit: make_singular(
                "msg::bad-icu",
                "Hello {0}",
                Some("Hallo {0"),
                UnitState::Proposed,
            ),
            locale: de_de(),
            // Unparseable target also yields a placeholder mismatch
            // because the multiset cannot be extracted from a non-message.
            expected: &[Flag::IcuParseError],
            forbidden: &[],
        },
        Fixture {
            name: "icu-parse/negative/valid-plural-inline",
            unit: make_singular(
                "msg::ok-plural",
                "Hello",
                Some("{count, plural, one {Eins} other {Andere}}"),
                UnitState::Proposed,
            ),
            locale: de_de(),
            // The {count} target multiset will not match an empty source
            // multiset, so flag placeholder mismatch — that's fine; we
            // assert only that ICU parse does NOT fire here.
            expected: &[Flag::PlaceholderMismatch],
            forbidden: &[Flag::IcuParseError],
        },
        Fixture {
            name: "empty-when-finished/positive/singular-none",
            unit: make_singular("msg::empty-finished", "Hello", None, UnitState::Finished),
            locale: de_de(),
            expected: &[Flag::EmptyTargetWhenFinished],
            forbidden: &[],
        },
        Fixture {
            name: "empty-when-finished/positive/plural-with-hole",
            unit: make_plural(
                "msg::plural-hole",
                "{count} unread message(s)",
                vec![Some("{count} ungelesene Nachricht"), None],
                UnitState::Finished,
            ),
            locale: de_de(),
            expected: &[Flag::EmptyTargetWhenFinished],
            forbidden: &[],
        },
        Fixture {
            name: "empty-when-finished/negative/singular-empty-string-ok",
            unit: make_singular("msg::empty-str", "x", Some(""), UnitState::Finished),
            locale: de_de(),
            expected: &[],
            forbidden: &[Flag::EmptyTargetWhenFinished],
        },
        Fixture {
            name: "empty-when-finished/negative/unfinished-state-not-checked",
            unit: make_singular(
                "msg::unfinished-none",
                "Hello",
                None,
                UnitState::Untranslated,
            ),
            locale: de_de(),
            expected: &[],
            forbidden: &[Flag::EmptyTargetWhenFinished],
        },
        Fixture {
            name: "accel-mismatch/positive/missing-in-target",
            unit: make_singular("msg::file", "&File", Some("Datei"), UnitState::Proposed),
            locale: de_de(),
            expected: &[Flag::AccelMismatch],
            forbidden: &[],
        },
        Fixture {
            name: "accel-mismatch/negative/escape-not-counted",
            unit: make_singular(
                "msg::escape",
                "Save && exit",
                Some("Speichern && beenden"),
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[],
            forbidden: &[Flag::AccelMismatch],
        },
        Fixture {
            name: "accel-mismatch/negative/both-present",
            unit: make_singular("msg::file-ok", "&File", Some("&Datei"), UnitState::Proposed),
            locale: de_de(),
            expected: &[],
            forbidden: &[Flag::AccelMismatch],
        },
        Fixture {
            name: "length-warn/positive/long-german",
            unit: make_singular(
                "msg::long",
                "Open file",
                Some("Speichern und Sicherheitskopie anlegen wirklich jetzt"),
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[Flag::LengthWarn],
            forbidden: &[],
        },
        Fixture {
            name: "length-warn/negative/within-threshold",
            unit: make_singular(
                "msg::short",
                "Save the document",
                Some("Dokument speichern"),
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[],
            forbidden: &[Flag::LengthWarn],
        },
        Fixture {
            // Short sources (< 8 chars) are exempt: one extra character in
            // "Red" → "Rojo" is 33% ratio noise, not a runaway translation.
            name: "length-warn/negative/short-source-exempt",
            unit: make_singular(
                "CColorCalibrationSupplierGuiComp::Red",
                "Red",
                Some("Rojo"),
                UnitState::Proposed,
            ),
            locale: es_es(),
            expected: &[],
            forbidden: &[Flag::LengthWarn],
        },
        Fixture {
            name: "cjk-punctuation/positive/full-width-comma-period",
            unit: make_singular(
                "msg::zh-greeting",
                "Hello, world.",
                Some("你好，世界。"),
                UnitState::Proposed,
            ),
            locale: zh_hans(),
            expected: &[Flag::CjkPunctuationTolerated],
            forbidden: &[],
        },
        Fixture {
            name: "cjk-punctuation/negative/not-han-script",
            unit: make_singular(
                "msg::de-greeting",
                "Hello, world.",
                Some("Hallo，Welt。"),
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[],
            forbidden: &[Flag::CjkPunctuationTolerated],
        },
        Fixture {
            name: "placeholder-agreement/positive/german-die-name",
            unit: make_singular(
                "msg::agreement",
                "Save {name}",
                Some("Speichern Sie die {name}"),
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[Flag::PlaceholderAgreementRisk],
            forbidden: &[],
        },
        Fixture {
            name: "placeholder-agreement/negative/no-determiner",
            unit: make_singular(
                "msg::no-agreement",
                "Save {name}",
                Some("Speichern Sie {name}"),
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[],
            forbidden: &[Flag::PlaceholderAgreementRisk],
        },
        Fixture {
            name: "placeholder-agreement/negative/word-not-determiner",
            unit: make_singular(
                "msg::no-det-word",
                "Save {name}",
                Some("Speichern Sie nun {name}"),
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[],
            forbidden: &[Flag::PlaceholderAgreementRisk],
        },
        Fixture {
            name: "placeholder-agreement/positive/german-contraction-zur",
            unit: make_singular(
                "msg::zur",
                "Go to {name}",
                Some("Wechseln zur {name}"),
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[Flag::PlaceholderAgreementRisk],
            forbidden: &[],
        },
        Fixture {
            name: "placeholder-agreement/positive/german-contraction-zum",
            unit: make_singular(
                "msg::zum",
                "Click {name}",
                Some("Zum {name} navigieren"),
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[Flag::PlaceholderAgreementRisk],
            forbidden: &[],
        },
        Fixture {
            name: "placeholder-agreement/positive/german-contraction-im",
            unit: make_singular(
                "msg::im",
                "In {place}",
                Some("Im {place} suchen"),
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[Flag::PlaceholderAgreementRisk],
            forbidden: &[],
        },
        Fixture {
            name: "placeholder-agreement/positive/spanish-contraction-del",
            unit: make_singular(
                "msg::del",
                "From {file}",
                Some("Importar del {file}"),
                UnitState::Proposed,
            ),
            locale: es_es(),
            expected: &[Flag::PlaceholderAgreementRisk],
            forbidden: &[],
        },
        Fixture {
            name: "placeholder-agreement/positive/spanish-contraction-al",
            unit: make_singular(
                "msg::al",
                "To {target}",
                Some("Ir al {target}"),
                UnitState::Proposed,
            ),
            locale: es_es(),
            expected: &[Flag::PlaceholderAgreementRisk],
            forbidden: &[],
        },
        Fixture {
            name: "clean/all-rules-quiet",
            unit: make_singular(
                "msg::clean",
                "Open {0}",
                Some("{0} öffnen"),
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[],
            forbidden: &[
                Flag::PlaceholderMismatch,
                Flag::PluralArityMismatch,
                Flag::IcuParseError,
                Flag::EmptyTargetWhenFinished,
                Flag::AccelMismatch,
                Flag::LengthWarn,
                Flag::CjkPunctuationTolerated,
                Flag::PlaceholderAgreementRisk,
                Flag::MarkupTagMismatch,
            ],
        },
        // ── markup-tag-mismatch ─────────────────────────────────────────
        Fixture {
            name: "markup-tag/positive/b-dropped",
            unit: make_singular(
                "msg::dropped",
                "Click <b>Save</b> to continue.",
                Some("Klicken Sie auf Speichern, um fortzufahren."),
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[Flag::MarkupTagMismatch],
            forbidden: &[],
        },
        Fixture {
            name: "markup-tag/positive/strong-instead-of-b",
            unit: make_singular(
                "msg::renamed",
                "Click <b>Save</b>.",
                Some("Klicken Sie auf <strong>Speichern</strong>."),
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[Flag::MarkupTagMismatch],
            forbidden: &[],
        },
        Fixture {
            name: "markup-tag/negative/preserved-identical",
            unit: make_singular(
                "msg::preserved",
                "Click <b>Save</b> to continue.",
                Some("Klicken Sie auf <b>Speichern</b>, um fortzufahren."),
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[],
            forbidden: &[Flag::MarkupTagMismatch],
        },
        Fixture {
            name: "markup-tag/negative/case-and-attribute-insensitive",
            unit: make_singular(
                "msg::attrs",
                "See <a href=\"/help\">docs</a>",
                // Same `a` tag name; attributes differ; case differs.
                Some("Siehe <A HREF=\"/de/hilfe\">Dokumente</A>"),
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[],
            forbidden: &[Flag::MarkupTagMismatch],
        },
        Fixture {
            name: "markup-tag/negative/source-has-no-tags",
            unit: make_singular(
                "msg::no-tags",
                "Simple message",
                // Even a stray `<` in the target shouldn't flag when source
                // had no tags — see comment in `markup_tag_check`.
                Some("Einfache Nachricht mit 1 < 2"),
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[],
            forbidden: &[Flag::MarkupTagMismatch],
        },
        Fixture {
            name: "markup-tag/negative/literal-angle-bracket-label",
            unit: make_singular(
                "msg::literal-label",
                // `<No ID>` is a literal UI label, not markup: `no` is not a
                // recognized tag name, so translating it to `<sin ID>` must
                // not register as a tag mismatch.
                "<No ID>",
                Some("<sin ID>"),
                UnitState::Proposed,
            ),
            locale: de_de(),
            expected: &[],
            forbidden: &[Flag::MarkupTagMismatch],
        },
    ];

    run_fixtures(table);
}
