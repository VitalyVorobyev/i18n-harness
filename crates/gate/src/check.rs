//! Rule evaluation. One pure entry point [`run`] that calls every rule in
//! a fixed order and assembles the [`crate::GateReport`].
//!
//! # Ordering
//!
//! Rules are evaluated in this order:
//!
//! 1. Plural arity (also catches `wrong_variant`).
//! 2. ICU parse — produces the per-slot placeholder multisets needed by #3.
//! 3. Placeholder multiset.
//! 4. Non-empty when finished.
//! 5. Accelerator.
//! 6. Length warn.
//! 7. CJK punctuation.
//! 8. Placeholder agreement risk.
//! 9. Markup tag preservation.
//!
//! Each rule appends zero or more [`crate::Finding`]s to the running list.
//! Rules do **not** short-circuit each other — if ICU parse fails for one
//! plural form we still try to check the others. The exception is the
//! placeholder multiset, which depends on a successful ICU parse of each
//! slot we want to compare against; for unparseable slots we record only
//! the parse error.

use i18n_harness_core::{Flag, Target, Unit, UnitState};
use i18n_harness_glossary::Glossary;
use i18n_harness_locales::{Locale, Register, Script};

use crate::icu::{Message, parse, placeholder_multiset};
use crate::report::{
    AccelDetail, CjkPunctuationDetail, EmptyTargetDetail, Finding, FindingDetail, GateReport,
    IcuParseDetail, LengthWarnDetail, MarkupTagMismatchDetail, PlaceholderAgreementDetail,
    PlaceholderMismatchDetail, PluralArityMismatchDetail,
};

/// Entry point used by [`crate::validate`].
pub(crate) fn run(unit: &Unit, locale: &Locale, _glossary: Option<&Glossary>) -> GateReport {
    let mut findings: Vec<Finding> = Vec::new();

    // Source ICU parse: best-effort. The source is the reference; if the
    // adapter produced unparseable ICU on the source side it is an adapter
    // bug, not the gate's concern. Downstream rules that need a source
    // multiset will skip themselves when `source_msg` is `None`.
    let source_msg = parse(&unit.source).ok();

    let target_slots = collect_target_slots(unit);

    arity_check(unit, locale, &mut findings);
    let parsed_targets = parse_targets(&target_slots, &mut findings);

    if let Some(source) = source_msg.as_ref() {
        placeholder_check(source, &parsed_targets, &mut findings);
    }

    empty_when_finished_check(unit, &mut findings);
    accel_check(unit, &target_slots, &mut findings);
    length_warn_check(unit, locale, &target_slots, &mut findings);
    cjk_punctuation_check(locale, &target_slots, &mut findings);
    placeholder_agreement_check(locale, &target_slots, &mut findings);
    markup_tag_check(unit, &target_slots, &mut findings);

    GateReport::from_findings(unit.id.clone(), findings)
}

/// One filled target slot. `slot` is the form index for plural units (0 for
/// singular).
struct TargetSlot<'a> {
    slot: u32,
    text: &'a str,
}

fn collect_target_slots(unit: &Unit) -> Vec<TargetSlot<'_>> {
    match &unit.target {
        Target::Singular { text: Some(t) } => vec![TargetSlot { slot: 0, text: t }],
        Target::Singular { text: None } => Vec::new(),
        Target::Plural { forms } => forms
            .iter()
            .enumerate()
            .filter_map(|(i, form)| {
                form.as_deref().map(|t| TargetSlot {
                    slot: i as u32,
                    text: t,
                })
            })
            .collect(),
    }
}

/// Parsed message per slot. `msg = None` means the slot's ICU was
/// unparseable and a finding has already been recorded.
struct ParsedTarget {
    slot: u32,
    msg: Option<Message>,
}

fn parse_targets(slots: &[TargetSlot<'_>], findings: &mut Vec<Finding>) -> Vec<ParsedTarget> {
    slots
        .iter()
        .map(|s| match parse(s.text) {
            Ok(m) => ParsedTarget {
                slot: s.slot,
                msg: Some(m),
            },
            Err(err) => {
                findings.push(Finding {
                    flag: Flag::IcuParseError,
                    detail: FindingDetail::IcuParseError(IcuParseDetail::from_parse(s.slot, err)),
                });
                ParsedTarget {
                    slot: s.slot,
                    msg: None,
                }
            }
        })
        .collect()
}

// ── Hard checks ───────────────────────────────────────────────────────────

fn arity_check(unit: &Unit, locale: &Locale, findings: &mut Vec<Finding>) {
    if unit.plural_arity.is_none() {
        return;
    }
    let expected = locale.plural_arity();
    match &unit.target {
        Target::Plural { forms } => {
            let found = forms.len() as u32;
            if found != expected {
                findings.push(Finding {
                    flag: Flag::PluralArityMismatch,
                    detail: FindingDetail::PluralArityMismatch(PluralArityMismatchDetail {
                        expected,
                        found,
                        wrong_variant: false,
                    }),
                });
            }
        }
        Target::Singular { .. } => {
            findings.push(Finding {
                flag: Flag::PluralArityMismatch,
                detail: FindingDetail::PluralArityMismatch(PluralArityMismatchDetail {
                    expected,
                    found: 1,
                    wrong_variant: true,
                }),
            });
        }
    }
}

fn placeholder_check(source: &Message, targets: &[ParsedTarget], findings: &mut Vec<Finding>) {
    let source_multiset = placeholder_multiset(source);
    for t in targets {
        let Some(ref msg) = t.msg else {
            continue;
        };
        let target_multiset = placeholder_multiset(msg);
        if source_multiset == target_multiset {
            continue;
        }
        let (missing, extra) = diff_multisets(&source_multiset, &target_multiset);
        findings.push(Finding {
            flag: Flag::PlaceholderMismatch,
            detail: FindingDetail::PlaceholderMismatch(PlaceholderMismatchDetail {
                slot: t.slot,
                missing,
                extra,
            }),
        });
    }
}

/// Tokens in `a` but not in `b` (counting multiplicity), and vice versa.
///
/// Both inputs are sorted, so we walk them in parallel — O(n + m), one
/// allocation per output. No HashMap; placeholder counts are tiny.
fn diff_multisets(a: &[String], b: &[String]) -> (Vec<String>, Vec<String>) {
    let mut missing = Vec::new();
    let mut extra = Vec::new();
    let mut i = 0;
    let mut j = 0;
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Equal => {
                i += 1;
                j += 1;
            }
            std::cmp::Ordering::Less => {
                missing.push(a[i].clone());
                i += 1;
            }
            std::cmp::Ordering::Greater => {
                extra.push(b[j].clone());
                j += 1;
            }
        }
    }
    missing.extend(a[i..].iter().cloned());
    extra.extend(b[j..].iter().cloned());
    (missing, extra)
}

fn empty_when_finished_check(unit: &Unit, findings: &mut Vec<Finding>) {
    if unit.state != UnitState::Finished {
        return;
    }
    let push_empty = |slot: u32, findings: &mut Vec<Finding>| {
        findings.push(Finding {
            flag: Flag::EmptyTargetWhenFinished,
            detail: FindingDetail::EmptyTargetWhenFinished(EmptyTargetDetail { slot }),
        });
    };
    match &unit.target {
        Target::Singular { text: None } => push_empty(0, findings),
        Target::Plural { forms } => {
            for (i, form) in forms.iter().enumerate() {
                if form.is_none() {
                    push_empty(i as u32, findings);
                }
            }
        }
        Target::Singular { text: Some(_) } => {}
    }
}

// ── Soft checks ───────────────────────────────────────────────────────────

/// Count literal `&` characters, treating `&&` as an escape that yields
/// no accelerator. Walks bytes; safe because `&` is single-byte ASCII.
fn count_accelerators(s: &str) -> u32 {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut count = 0u32;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            if bytes.get(i + 1) == Some(&b'&') {
                // Escaped `&&`; not an accelerator.
                i += 2;
                continue;
            }
            count += 1;
        }
        i += 1;
    }
    count
}

fn accel_check(unit: &Unit, slots: &[TargetSlot<'_>], findings: &mut Vec<Finding>) {
    if slots.is_empty() {
        return;
    }
    let source = count_accelerators(&unit.source);
    let target: u32 = slots.iter().map(|s| count_accelerators(s.text)).sum();
    if source != target {
        findings.push(Finding {
            flag: Flag::AccelMismatch,
            detail: FindingDetail::AccelMismatch(AccelDetail {
                source_count: source,
                target_count: target,
            }),
        });
    }
}

fn length_warn_check(
    unit: &Unit,
    locale: &Locale,
    slots: &[TargetSlot<'_>],
    findings: &mut Vec<Finding>,
) {
    if slots.is_empty() {
        return;
    }
    // char count, not byte count — fair across scripts.
    let source_chars = unit.source.chars().count() as u32;
    if source_chars == 0 {
        return;
    }
    // At very short lengths a single extra character swings the ratio by
    // 25–50%, producing false positives for trivially correct translations
    // (e.g. "Red" → "Rojo" = 1.33×, "Test" → "Probar" = 1.5×). Ratio
    // noise dominates meaning below 8 source characters, so skip the check.
    if source_chars < 8 {
        return;
    }
    let target_chars: u32 = slots.iter().map(|s| s.text.chars().count() as u32).sum();
    let threshold = locale.length_warn_ratio;
    // UI strings are tiny (a screen at a time); even at 100 KB per slot
    // the loss of precision converting u32 → f32 is irrelevant for a
    // 2-significant-figures ratio test.
    let ratio = target_chars as f32 / source_chars as f32;
    if ratio > threshold {
        findings.push(Finding {
            flag: Flag::LengthWarn,
            detail: FindingDetail::LengthWarn(LengthWarnDetail {
                source_chars,
                target_chars,
                threshold,
                ratio,
            }),
        });
    }
}

/// ASCII → full-width mapping for the punctuation we care about.
/// Mapping table keeps it ASCII-aware: if the source uses any of these
/// ASCII characters and the target uses the full-width counterpart, we
/// flag (informational, expected convention).
const PUNCT_PAIRS: &[(char, char)] = &[
    (',', '\u{FF0C}'), // ，
    ('.', '\u{3002}'), // 。
    ('!', '\u{FF01}'), // ！
    ('?', '\u{FF1F}'), // ？
    (':', '\u{FF1A}'), // ：
    (';', '\u{FF1B}'), // ；
];

fn cjk_punctuation_check(locale: &Locale, slots: &[TargetSlot<'_>], findings: &mut Vec<Finding>) {
    if locale.script != Script::Han || slots.is_empty() {
        return;
    }
    let mut seen: Vec<char> = Vec::new();
    for slot in slots {
        for (_ascii, full) in PUNCT_PAIRS {
            if slot.text.contains(*full) && !seen.contains(full) {
                seen.push(*full);
            }
        }
    }
    if !seen.is_empty() {
        seen.sort();
        findings.push(Finding {
            flag: Flag::CjkPunctuationTolerated,
            detail: FindingDetail::CjkPunctuationTolerated(CjkPunctuationDetail {
                characters: seen,
            }),
        });
    }
}

/// Determiners that, immediately preceding a placeholder in the target,
/// make gender/case agreement undeterminable at translation time.
/// Lowercased; matched case-insensitively.
///
/// The German list also covers common preposition+article contractions
/// (`zum`/`zur`/`im`/`vom`/`am`/`beim`) — these stand in for `dem`/`der`
/// in real UI strings and produce the same agreement problem. The Spanish
/// list covers `del` (de+el) and `al` (a+el) for the same reason.
const DETERMINERS: &[&str] = &[
    // German — definite/indefinite articles (nominative/accusative/dative/genitive)
    "der", "die", "das", "den", "dem", "des", "ein", "eine", "einen", "einem", "einer", "eines",
    // German — preposition+article contractions (very common in UI strings)
    "zum", "zur", "im", "vom", "am", "beim", "ans", "ins",
    // Spanish — definite/indefinite articles
    "el", "la", "los", "las", "un", "una", "unos", "unas",
    // Spanish — preposition+article contractions
    "del", "al",
];

fn placeholder_agreement_check(
    locale: &Locale,
    slots: &[TargetSlot<'_>],
    findings: &mut Vec<Finding>,
) {
    if locale.script != Script::Latin {
        return;
    }
    if !matches!(locale.register, Register::Formal | Register::Informal) {
        return;
    }
    for slot in slots {
        scan_agreement_risks(slot.text, findings);
    }
}

fn scan_agreement_risks(text: &str, findings: &mut Vec<Finding>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            // Find the closing brace; if missing, skip (gate's ICU parse
            // already flagged it).
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b'}' {
                j += 1;
            }
            if j >= bytes.len() {
                return;
            }
            // Only flag for plain `{name}` or `{0}` style; opaque
            // formatted placeholders (`{x, number, …}`) still count as
            // placeholders for agreement purposes, so we include them too.
            let inside = &text[i + 1..j];
            let name = first_name_segment(inside);
            if name.is_empty() {
                i = j + 1;
                continue;
            }
            // Look at the word immediately before the `{`. Skip a single
            // space; reject if the previous non-space char is not part of
            // a word (we want adjacency, e.g. "die {name}").
            if let Some(det) = preceding_determiner(text, i) {
                findings.push(Finding {
                    flag: i18n_harness_core::Flag::PlaceholderAgreementRisk,
                    detail: FindingDetail::PlaceholderAgreementRisk(PlaceholderAgreementDetail {
                        placeholder: format!("{{{name}}}"),
                        determiner: det,
                    }),
                });
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
}

fn first_name_segment(inside: &str) -> &str {
    // `{name, plural, …}` → `name`. `{0}` → `0`. `{name}` → `name`.
    inside
        .split(|c: char| c == ',' || c.is_whitespace())
        .next()
        .unwrap_or("")
}

/// If the byte index `brace` is the position of a `{` and the immediately
/// preceding token (separated by a single ASCII space) is a determiner,
/// return it. Case-insensitive ASCII compare.
fn preceding_determiner(text: &str, brace: usize) -> Option<String> {
    if brace == 0 {
        return None;
    }
    let bytes = text.as_bytes();
    if bytes[brace - 1] != b' ' {
        return None;
    }
    // Walk back from brace-2 collecting ASCII letters.
    let end = brace - 1;
    let mut start = end;
    while start > 0 {
        let prev = bytes[start - 1];
        if prev.is_ascii_alphabetic() {
            start -= 1;
            continue;
        }
        break;
    }
    if start == end {
        return None;
    }
    // The word must be at the start of the text or preceded by whitespace
    // / punctuation that ends the previous lexical token — guard against
    // matching mid-word (e.g. "alldas" should not count).
    if start > 0 {
        let before = bytes[start - 1];
        if before.is_ascii_alphabetic() {
            return None;
        }
    }
    let word = std::str::from_utf8(&bytes[start..end]).ok()?;
    let lower = word.to_ascii_lowercase();
    if DETERMINERS.iter().any(|d| *d == lower) {
        Some(word.to_owned())
    } else {
        None
    }
}

/// Markup tag preservation check (soft).
///
/// HTML-style tags in UI strings are common (`<b>`, `<i>`, `<a href=...>`,
/// `<br/>`). When a model drops or reorders them, the rendered UI breaks.
/// The check compares the multiset of tag NAMES (`b`, `i`, `a`, …) between
/// source and target; attributes and case are ignored, self-closing
/// `<br/>` is treated identically to `<br>`.
///
/// We **don't** parse arbitrary HTML — that is overkill for the UI-string
/// case. We just lex `<NAME>` / `</NAME>` / `<NAME ... />` patterns. This
/// is intentionally lenient on attribute syntax and intentionally strict
/// on tag names: if the source uses `<b>` and the target uses `<strong>`,
/// that is a tag-name mismatch worth flagging.
fn markup_tag_check(unit: &Unit, slots: &[TargetSlot<'_>], findings: &mut Vec<Finding>) {
    if slots.is_empty() {
        return;
    }
    let source_tags = extract_tag_names(&unit.source);
    if source_tags.is_empty() {
        // No tags in source → don't flag spurious tag insertions in the
        // target. Spurious `<` in target text would surface as an
        // accelerator/length issue or be visible to the reviewer; flagging
        // every model-introduced `<` would be too noisy.
        return;
    }
    for slot in slots {
        let target_tags = extract_tag_names(slot.text);
        let (missing, extra) = multiset_diff(&source_tags, &target_tags);
        if missing.is_empty() && extra.is_empty() {
            continue;
        }
        findings.push(Finding {
            flag: Flag::MarkupTagMismatch,
            detail: FindingDetail::MarkupTagMismatch(MarkupTagMismatchDetail {
                slot: slot.slot,
                missing,
                extra,
            }),
        });
    }
}

/// HTML / Qt-rich-text tag names recognized as markup. Restricting the lexer
/// to this set is what keeps literal angle-bracket labels — `<No ID>`,
/// `<empty>`, `<unset>` and the like, common in UI source strings — from being
/// mistaken for markup and flagged as a tag mismatch against their (equally
/// literal) translation. The list is the Qt rich-text HTML subset plus the
/// inline/formatting tags that actually appear in UI strings; it is matched
/// case-insensitively (names are lower-cased before lookup).
const MARKUP_TAG_NAMES: &[&str] = &[
    "a",
    "abbr",
    "b",
    "big",
    "blockquote",
    "body",
    "br",
    "center",
    "cite",
    "code",
    "dd",
    "dfn",
    "div",
    "dl",
    "dt",
    "em",
    "font",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "hr",
    "html",
    "i",
    "img",
    "kbd",
    "li",
    "nobr",
    "ol",
    "p",
    "pre",
    "q",
    "s",
    "samp",
    "small",
    "span",
    "strong",
    "sub",
    "sup",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "title",
    "tr",
    "tt",
    "u",
    "ul",
    "var",
];

/// Lex `<NAME>`, `</NAME>`, and `<NAME ... />` patterns out of `text` and
/// return the tag names in document order. Lower-cases names so `<B>` and
/// `<b>` collapse. Only names in [`MARKUP_TAG_NAMES`] count; any other
/// `<word …>` is treated as literal text, not markup.
///
/// Heuristics:
/// - A `<` followed by `/`, an ASCII letter, or `_` starts a candidate.
/// - The name runs while characters are ASCII alphanumeric, `-`, or `_`.
/// - Anything else (or end-of-string before `>`) aborts the candidate;
///   the lone `<` is not counted.
/// - Self-closing `<br/>` and opening `<br>` produce the same name; we do
///   not distinguish them at this granularity.
fn extract_tag_names(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        if j < bytes.len() && bytes[j] == b'/' {
            j += 1;
        }
        let name_start = j;
        while j < bytes.len() {
            let b = bytes[j];
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
                j += 1;
            } else {
                break;
            }
        }
        if j == name_start {
            // No name characters after `<` (or `</`) — not a tag.
            i += 1;
            continue;
        }
        // Now skip attributes / whitespace up to `>`. If we hit end-of-
        // string without finding `>`, treat as not-a-tag.
        let mut k = j;
        while k < bytes.len() && bytes[k] != b'>' {
            k += 1;
        }
        if k >= bytes.len() {
            i += 1;
            continue;
        }
        if let Ok(name) = std::str::from_utf8(&bytes[name_start..j]) {
            let lowered = name.to_ascii_lowercase();
            if MARKUP_TAG_NAMES.contains(&lowered.as_str()) {
                out.push(lowered);
            }
        }
        i = k + 1;
    }
    out
}

/// Compute (missing-in-target, extra-in-target) multisets. Each side may
/// contain duplicates (e.g. two `<b>` in source, one in target → `b` in
/// missing).
fn multiset_diff(source: &[String], target: &[String]) -> (Vec<String>, Vec<String>) {
    let mut s: Vec<String> = source.to_vec();
    let mut t: Vec<String> = target.to_vec();
    s.sort();
    t.sort();
    let mut missing = Vec::new();
    let mut extra = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < s.len() && j < t.len() {
        match s[i].cmp(&t[j]) {
            std::cmp::Ordering::Equal => {
                i += 1;
                j += 1;
            }
            std::cmp::Ordering::Less => {
                missing.push(s[i].clone());
                i += 1;
            }
            std::cmp::Ordering::Greater => {
                extra.push(t[j].clone());
                j += 1;
            }
        }
    }
    missing.extend_from_slice(&s[i..]);
    extra.extend_from_slice(&t[j..]);
    (missing, extra)
}
