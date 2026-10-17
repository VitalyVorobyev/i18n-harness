//! Gate findings in a translator's words.

use i18n_harness_core::{Flag, FlagSeverity, Unit};
use i18n_harness_formats::CatalogFormat;
use i18n_harness_gate::{FindingDetail, GateReport};

use crate::view::{Issue, IssueSeverity};

/// Turn a gate report (plus the model's own remarks on `unit`) into issues.
pub(crate) fn issues(report: &GateReport, unit: &Unit, format: &dyn CatalogFormat) -> Vec<Issue> {
    let native = |icu: &str| {
        format
            .placeholders_from_icu(icu)
            .unwrap_or_else(|_| icu.to_owned())
    };
    let list = |items: &[String]| {
        items
            .iter()
            .map(|p| native(p))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let form = |slot: u32, plural: bool| {
        if plural {
            format!(" (form {})", slot + 1)
        } else {
            String::new()
        }
    };
    let plural = unit.plural_arity.is_some();

    let mut out: Vec<Issue> = report
        .findings
        .iter()
        .filter_map(|f| {
            let (severity, message) = match &f.detail {
                FindingDetail::PlaceholderMismatch(d) => {
                    let mut parts = Vec::new();
                    if !d.missing.is_empty() {
                        parts.push(format!("missing {}", list(&d.missing)));
                    }
                    if !d.extra.is_empty() {
                        parts.push(format!("unexpected {}", list(&d.extra)));
                    }
                    (
                        IssueSeverity::Error,
                        format!(
                            "Placeholders do not match the original{}: {}",
                            form(d.slot, plural),
                            parts.join("; ")
                        ),
                    )
                }
                FindingDetail::PluralArityMismatch(d) if d.wrong_variant => (
                    IssueSeverity::Error,
                    format!("This string needs {} plural forms", d.expected),
                ),
                FindingDetail::PluralArityMismatch(d) => (
                    IssueSeverity::Error,
                    format!(
                        "This string needs {} plural forms, the translation has {}",
                        d.expected, d.found
                    ),
                ),
                FindingDetail::IcuParseError(d) => (
                    IssueSeverity::Error,
                    format!(
                        "The translation has unbalanced braces or a broken placeholder{}: {}",
                        form(d.slot, plural),
                        d.message
                    ),
                ),
                FindingDetail::EmptyTargetWhenFinished(d) => (
                    IssueSeverity::Error,
                    format!(
                        "Marked as done but the translation is empty{}",
                        form(d.slot, plural)
                    ),
                ),
                FindingDetail::BackendMalformedResponse(d) => (
                    IssueSeverity::Error,
                    format!("The model's answer could not be used: {}", d.reason),
                ),
                FindingDetail::AccelMismatch(d) => (
                    IssueSeverity::Warning,
                    format!(
                        "Keyboard shortcut marker (&) appears {} time(s) in the original and {} in the translation",
                        d.source_count, d.target_count
                    ),
                ),
                FindingDetail::LengthWarn(d) => (
                    IssueSeverity::Warning,
                    format!(
                        "The translation is much longer than the original ({} vs {} characters) and may not fit",
                        d.target_chars, d.source_chars
                    ),
                ),
                FindingDetail::MarkupTagMismatch(d) => {
                    let mut parts = Vec::new();
                    if !d.missing.is_empty() {
                        parts.push(format!("missing {}", d.missing.join(", ")));
                    }
                    if !d.extra.is_empty() {
                        parts.push(format!("unexpected {}", d.extra.join(", ")));
                    }
                    (
                        IssueSeverity::Warning,
                        format!(
                            "Formatting tags do not match the original{}: {}",
                            form(d.slot, plural),
                            parts.join("; ")
                        ),
                    )
                }
                FindingDetail::PlaceholderAgreementRisk(d) => (
                    IssueSeverity::Warning,
                    format!(
                        "\"{}\" before {} may not agree with every value it can take",
                        d.determiner,
                        native(&d.placeholder)
                    ),
                ),
                // Expected for CJK targets; not worth a translator's attention.
                FindingDetail::CjkPunctuationTolerated(_) => return None,
            };
            Some(Issue { severity, message })
        })
        .collect();

    // The model's remarks about its own output.
    for flag in unit.flags.iter() {
        if flag.severity() != FlagSeverity::Semantic {
            continue;
        }
        let note = unit.flag_notes.get(&flag).map(String::as_str);
        let what = match flag {
            Flag::AmbiguousSource => "The original is ambiguous",
            Flag::Idiom => "The original contains an idiom",
            Flag::InsufficientContext => "The model lacked context",
            Flag::LowConfidence => "The model was unsure",
            Flag::BrandTerm => "Contains a brand or product name",
            Flag::ToneMismatch => "The tone may not match",
            _ => continue,
        };
        out.push(Issue {
            severity: IssueSeverity::Note,
            message: match note {
                Some(note) if !note.is_empty() => format!("{what}: {note}"),
                _ => what.to_owned(),
            },
        });
    }
    out
}
