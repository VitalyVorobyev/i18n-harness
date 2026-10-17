//! Integration test running the gate over the Qt showcase fixture.
//!
//! Two passes:
//!
//! 1. **As-extracted.** The showcase has no filled targets and every unit is
//!    `Untranslated`. The gate should produce **no hard flags** (the empty
//!    state is fine — it just means there is nothing to ship yet).
//! 2. **Deliberately corrupted.** We mutate units in known ways (rotate a
//!    placeholder, drop one plural form, inject malformed ICU) and verify
//!    the gate produces exactly the expected hard flags.

use std::path::PathBuf;

use i18n_harness_adapter_qt::extract;
use i18n_harness_core::{Flag, Target, UnitState};
use i18n_harness_gate::validate_batch;
use i18n_harness_locales::Locale;

fn showcase_path() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    std::path::Path::new(manifest)
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root")
        .join("fixtures")
        .join("qt")
        .join("showcase.ts")
}

#[test]
fn untranslated_showcase_produces_no_hard_flags() {
    let catalog = extract(&showcase_path()).expect("extract showcase");
    let de = Locale::by_id("de_DE").expect("de_DE");
    let reports = validate_batch(catalog.units(), de, None);

    let mut offenders = Vec::new();
    for (unit, report) in catalog.units().iter().zip(&reports) {
        // `Untranslated`/`Proposed`/`Vanished`/`Obsolete` all have either
        // no target text or text the harness must not touch. None of those
        // should produce hard flags from the gate's hard checks.
        if report.has_hard() {
            offenders.push(format!(
                "[{}] state={:?} hard={:?}",
                unit.id, unit.state, report.flags
            ));
        }
    }
    assert!(
        offenders.is_empty(),
        "untranslated showcase produced hard flags:\n{}",
        offenders.join("\n"),
    );
}

#[test]
fn corrupting_a_singular_target_produces_placeholder_mismatch() {
    let catalog = extract(&showcase_path()).expect("extract");
    let de = Locale::by_id("de_DE").expect("de_DE");
    let mut units = catalog.units().to_vec();

    let target = units
        .iter_mut()
        .find(|u| u.source == "Open {0} from {1}")
        .expect("Open {0} from {1} unit must be present in showcase");
    // Drop `{1}` to provoke the multiset check.
    target.target = Target::Singular {
        text: Some("Datei {0} öffnen".into()),
    };
    target.state = UnitState::Proposed;

    let reports = validate_batch(&units, de, None);
    // The unit id is built from the pre-normalized Qt source text, so the
    // id contains `%1`/`%2`; find the report by source text instead.
    let target_id = units
        .iter()
        .find(|u| u.source == "Open {0} from {1}")
        .map(|u| u.id.clone())
        .expect("Open unit must exist");
    let report = reports
        .iter()
        .find(|r| r.unit_id == target_id)
        .expect("report for corrupted unit");
    assert!(
        report.flags.contains(Flag::PlaceholderMismatch),
        "expected placeholder mismatch, got {:?}",
        report.flags,
    );
}

#[test]
fn dropping_a_plural_form_produces_arity_mismatch() {
    let catalog = extract(&showcase_path()).expect("extract");
    let de = Locale::by_id("de_DE").expect("de_DE");
    let mut units = catalog.units().to_vec();

    // The plural unit in showcase has source `"%n unread message(s)"`,
    // which normalizes to `"{count} unread message(s)"` after the Qt
    // placeholder converter runs on extract.
    let target = units
        .iter_mut()
        .find(|u| u.plural_arity.is_some())
        .expect("plural unit must be present in showcase");
    target.target = Target::Plural {
        forms: vec![Some("{count} ungelesene Nachricht".into())],
    };
    target.state = UnitState::Proposed;

    let reports = validate_batch(&units, de, None);
    let target_id = units
        .iter()
        .find(|u| u.plural_arity.is_some())
        .map(|u| u.id.clone())
        .expect("plural unit");
    let report = reports
        .iter()
        .find(|r| r.unit_id == target_id)
        .expect("report for plural unit");
    assert!(
        report.flags.contains(Flag::PluralArityMismatch),
        "expected plural arity mismatch, got {:?}",
        report.flags,
    );
}

#[test]
fn injecting_malformed_icu_produces_parse_error() {
    let catalog = extract(&showcase_path()).expect("extract");
    let de = Locale::by_id("de_DE").expect("de_DE");
    let mut units = catalog.units().to_vec();

    let target = units
        .iter_mut()
        .find(|u| u.source == "Hello")
        .expect("Hello unit must be present in showcase");
    // Unbalanced `{` — pure ICU parse failure.
    target.target = Target::Singular {
        text: Some("Hallo {0".into()),
    };
    target.state = UnitState::Proposed;

    let reports = validate_batch(&units, de, None);
    let target_id = units
        .iter()
        .find(|u| u.source == "Hello")
        .map(|u| u.id.clone())
        .expect("Hello unit");
    let report = reports
        .iter()
        .find(|r| r.unit_id == target_id)
        .expect("report for Hello");
    assert!(
        report.flags.contains(Flag::IcuParseError),
        "expected ICU parse error, got {:?}",
        report.flags,
    );
}

#[test]
fn marking_finished_without_text_produces_empty_target_flag() {
    let catalog = extract(&showcase_path()).expect("extract");
    let de = Locale::by_id("de_DE").expect("de_DE");
    let mut units = catalog.units().to_vec();

    // Find any `Untranslated` singular unit and flip its state to
    // `Finished` without filling the target.
    let target = units
        .iter_mut()
        .find(|u| u.state == UnitState::Untranslated && matches!(u.target, Target::Singular { .. }))
        .expect("at least one untranslated singular unit");
    target.state = UnitState::Finished;

    let reports = validate_batch(&units, de, None);
    let offender = reports
        .iter()
        .find(|r| r.flags.contains(Flag::EmptyTargetWhenFinished));
    assert!(
        offender.is_some(),
        "expected at least one EmptyTargetWhenFinished flag",
    );
}

fn sk_plural_unit() -> i18n_harness_core::Unit {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/qt/sk_SK_pivot.ts");
    extract(&path)
        .expect("extract sk fixture")
        .units()
        .iter()
        .find(|u| u.plural_arity.is_some())
        .expect("plural unit")
        .clone()
}

fn arity_flagged(unit: &i18n_harness_core::Unit, plural_forms: Option<u32>) -> bool {
    let ctx = i18n_harness_gate::GateContext {
        locale: Locale::by_id("sk_SK").expect("sk_SK"),
        glossary: None,
        plural_forms,
    };
    i18n_harness_gate::validate_with(unit, &ctx)
        .flags
        .contains(Flag::PluralArityMismatch)
}

fn with_forms(n: usize) -> i18n_harness_core::Unit {
    let mut unit = sk_plural_unit();
    unit.target = Target::Plural {
        forms: (0..n).map(|i| Some(format!("{{count}} f{i}"))).collect(),
    };
    unit.state = UnitState::Proposed;
    unit
}

#[test]
fn slovak_qt_plural_with_three_forms_passes_without_file_evidence() {
    // CLDR has four Slovak categories, but Qt's (and gettext's) table has
    // three; `many` covers fractions only.
    assert!(!arity_flagged(&with_forms(3), None));
    assert!(arity_flagged(&with_forms(4), None));
}

#[test]
fn file_declared_plural_count_overrides_the_locale_default() {
    assert!(!arity_flagged(&with_forms(4), Some(4)));
    assert!(arity_flagged(&with_forms(3), Some(4)));
}

#[test]
fn keep_unchanged_glossary_terms_must_survive_translation() {
    let (glossary, _) = i18n_harness_glossary::Glossary::from_toml(
        "[meta]\nschema_version = 1\n\n[[term]]\nsource = \"Blob\"\ndo_not_translate = true\n\n\
         [[term]]\nsource = \"ParamsManager\"\ndo_not_translate = true\n",
    )
    .unwrap();
    let ctx = i18n_harness_gate::GateContext {
        locale: Locale::by_id("sk_SK").unwrap(),
        glossary: Some(&glossary),
        plural_forms: None,
    };
    let unit = |target: &str| {
        let mut u =
            i18n_harness_core::Unit::untranslated_singular("id", "Open ParamsManager for the Blob");
        u.target = Target::Singular {
            text: Some(target.into()),
        };
        u.state = UnitState::Proposed;
        u
    };
    let flagged = |t: &str| {
        i18n_harness_gate::validate_with(&unit(t), &ctx)
            .flags
            .contains(Flag::ProtectedTermMismatch)
    };
    assert!(!flagged("Otvoriť ParamsManager pre Blob"));
    assert!(
        flagged("Otvoriť správcu parametrov pre Blob"),
        "translated identifier"
    );
    assert!(
        flagged("Otvoriť ParamsManager pre Blobu"),
        "declined term is not the term"
    );
    // Terms absent from the source impose nothing.
    let mut other = unit("Uložiť");
    other.source = "Save".into();
    assert!(
        !i18n_harness_gate::validate_with(&other, &ctx)
            .flags
            .contains(Flag::ProtectedTermMismatch)
    );
}

#[test]
fn a_translated_icu_keyword_is_a_placeholder_mismatch() {
    let mut u = i18n_harness_core::Unit::untranslated_singular(
        "id",
        "{count, plural, one {# file} other {# files}}",
    );
    u.target = Target::Singular {
        text: Some("{count, pluriel, un {# fichier} other {# fichiers}}".into()),
    };
    u.state = UnitState::Proposed;
    let report = i18n_harness_gate::validate(&u, Locale::by_id("fr_FR").unwrap(), None);
    assert!(
        report.flags.contains(Flag::PlaceholderMismatch),
        "{report:?}"
    );
}

#[test]
fn plural_lengths_are_compared_form_by_form() {
    let mut u = i18n_harness_core::Unit::untranslated_singular("id", "{count} files were imported");
    u.plural_arity = Some(3);
    u.target = Target::Plural {
        forms: vec![
            Some("{count} súbor importovaný".into()),
            Some("{count} súbory importované".into()),
            Some("{count} súborov importovaných".into()),
        ],
    };
    u.state = UnitState::Proposed;
    let report = i18n_harness_gate::validate(&u, Locale::by_id("sk_SK").unwrap(), None);
    assert!(!report.flags.contains(Flag::LengthWarn), "{report:?}");
}
