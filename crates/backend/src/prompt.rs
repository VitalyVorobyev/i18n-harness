//! Lightweight prompt-template helper.
//!
//! Templates live in `crates/backend/prompts/` as plain text files named
//! `<backend>-<purpose>-v<N>.txt`. The version is part of the file name
//! *and* baked into the template body (as a `[template=...]` line) so a
//! metrics consumer can correlate "model X gave bad German" with "we used
//! template version Y at the time".
//!
//! # Why no `tera`/`handlebars`
//!
//! The substitution surface is tiny: half a dozen named tokens. A 30-line
//! helper that walks the template and replaces `{token}` occurrences with
//! values keeps the crate's dep graph minimal and the failure modes
//! local. If the prompt surface ever needs loops or conditionals we will
//! revisit; for now, every prompt is a fixed envelope with substituted
//! values.
//!
//! # Supported tokens (v1 vocabulary)
//!
//! | Token | Meaning |
//! |---|---|
//! | `{source}` | The unit's source text (ICU-normalized). |
//! | `{locale}` | The target locale id (`de_DE`). |
//! | `{register}` | The effective register: `formal`/`informal`/`neutral`. |
//! | `{glossary_block_or_(none)}` | A list of glossary entries for the target locale, one `<src> -> <tgt>` per line, or `"(none)"` if empty. The trailing `_or_(none)` is part of the token name and self-documents the empty-case substitution. |
//! | `{do_not_translate_block_or_(none)}` | A bullet list of DNT sources, or `"(none)"` if empty. |
//! | `{template_version}` | The template's declared version (string, e.g. `"v1"`). |
//! | `{locale_example_block_or_empty}` | A short block of two source/translation example pairs for the target locale (so the model sees real-language examples, not just structural ones). Empty when no curated examples exist for the locale. |
//! | `{plural_category_line_or_empty}` | When the caller is rendering one form of a plural unit, a directive naming the CLDR category, a numeric hint, and a reminder to drop English plural markers like `(s)`/`(es)`. Empty for singular renders. Driven by [`PromptContext::plural_category`]. |
//!
//! Unknown `{key}` patterns in the template are left **as-is** (verbatim)
//! in the rendered output. This is deliberate: prompt bodies routinely
//! contain instructional examples like `{count}` or `{{var}}` that are
//! NOT template tokens but illustrate ICU placeholder syntax for the
//! model. Mangling them would be wrong. Typos in token names are caught
//! by reading the rendered prompt — the missing substitution is visible
//! because the template's intended slot still carries the literal braces.

use std::collections::BTreeMap;

use crate::context::PromptContext;

/// A loaded prompt template plus its version tag.
///
/// Templates are usually loaded from `prompts/<backend>-<purpose>-v<N>.txt`
/// at startup, then reused across batches. They are cheap to clone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptTemplate {
    /// The template body, with `{token}` placeholders.
    body: String,
    /// Version string (e.g. `"v1"`). Surfaced into rendered prompts as
    /// `{template_version}`.
    version: String,
}

impl PromptTemplate {
    /// Construct from raw body and version. The version is interpolated
    /// into the body wherever `{template_version}` appears.
    pub fn new(body: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            body: body.into(),
            version: version.into(),
        }
    }

    /// Render the template against a [`PromptContext`].
    ///
    /// The result is a plain string the backend hands to the model.
    pub fn render(&self, ctx: &PromptContext<'_>) -> String {
        let mut tokens: BTreeMap<&str, String> = BTreeMap::new();
        tokens.insert("source", ctx.unit.source.clone());
        tokens.insert("locale", ctx.locale.id.to_owned());
        tokens.insert("register", register_str(ctx.register).to_owned());
        let glossary = render_glossary_block(ctx);
        let dnt = render_do_not_translate_block(ctx);
        // Both the bare names and the self-documenting `_or_(none)` aliases
        // resolve to the same value; the template author can pick whichever
        // reads better in context.
        tokens.insert("glossary_block", glossary.clone());
        tokens.insert("glossary_block_or_(none)", glossary);
        tokens.insert("do_not_translate_block", dnt.clone());
        tokens.insert("do_not_translate_block_or_(none)", dnt);
        tokens.insert("template_version", self.version.clone());
        tokens.insert(
            "locale_example_block_or_empty",
            render_locale_example_block(ctx),
        );
        tokens.insert(
            "plural_category_line_or_empty",
            render_plural_category_line(ctx),
        );
        substitute(&self.body, &tokens)
    }
}

fn register_str(r: i18n_harness_glossary::Register) -> &'static str {
    match r {
        i18n_harness_glossary::Register::Formal => "formal",
        i18n_harness_glossary::Register::Informal => "informal",
        i18n_harness_glossary::Register::Neutral => "neutral",
    }
}

fn render_glossary_block(ctx: &PromptContext<'_>) -> String {
    let lines: Vec<String> = ctx
        .glossary_terms()
        .map(|(src, tgt)| format!("{src} -> {tgt}"))
        .collect();
    if lines.is_empty() {
        "(none)".to_owned()
    } else {
        lines.join("\n")
    }
}

fn render_do_not_translate_block(ctx: &PromptContext<'_>) -> String {
    let lines: Vec<String> = ctx
        .do_not_translate()
        .map(|src| format!("- {src}"))
        .collect();
    if lines.is_empty() {
        "(none)".to_owned()
    } else {
        lines.join("\n")
    }
}

/// Per-locale curated example block. The block uses the same `⟦…⟧`
/// wrappers as the template's structural examples so the model sees a
/// consistent format. Returns the empty string for locales without a
/// curated block — the template's `_or_empty` suffix documents this.
///
/// The list is short on purpose: 2–3 representative pairs per locale.
/// Anything longer eats tokens we'd rather spend on the source itself.
fn render_locale_example_block(ctx: &PromptContext<'_>) -> String {
    let pairs: &[(&str, &str)] = match ctx.locale.id {
        "de_DE" => &[
            ("Save", "Speichern"),
            ("Cancel", "Abbrechen"),
            ("Open", "Öffnen"),
        ],
        "es_ES" => &[
            ("Save", "Guardar"),
            ("Cancel", "Cancelar"),
            ("Open", "Abrir"),
        ],
        "zh_Hans" => &[("Save", "保存"), ("Cancel", "取消"), ("Open", "打开")],
        _ => return String::new(),
    };
    let mut out = format!("Examples ({}):\n", ctx.locale.id);
    for (src, tgt) in pairs {
        out.push_str(&format!("SOURCE: ⟦{src}⟧\nTRANSLATION: {tgt}\n"));
    }
    out
}

/// CLDR plural-form directive. Empty for singular renders.
///
/// For plural renders, the line tells the model exactly which CLDR
/// category to produce AND explicitly forbids echoing the source's
/// plural-marker punctuation (`(s)`, `(es)`, `(en)`, etc.). Without the
/// "drop the marker" reminder, smaller Gemma builds tend to emit
/// `Nachricht(en)` for both the `one` and `other` forms, defeating the
/// CLDR distinction the gate is trying to validate.
fn render_plural_category_line(ctx: &PromptContext<'_>) -> String {
    let Some(cat) = ctx.plural_category else {
        return String::new();
    };
    let numeric_hint = match cat.to_string().as_str() {
        "zero" => " (count = 0)",
        "one" => " (count = 1, the singular)",
        "two" => " (count = 2, the dual)",
        "few" => " (small count: typically 2–4)",
        "many" => " (large count)",
        "other" => " (general plural, count > 1)",
        _ => "",
    };
    format!(
        "Plural form: produce the \"{cat}\"{numeric_hint} form for {locale}.\n\
         Drop ONLY parenthetical English plural suffixes like `(s)`, `(es)`, `(en)` — \
         they are hints, not literal text. \
         Always preserve the count placeholder `%n` or `{{count}}` exactly as it \
         appears in the source; never drop or rename it. \
         Output the natural {locale} word for this specific count.",
        locale = ctx.locale.id,
    )
}

/// Walk `body` and replace `{key}` occurrences with their values from
/// `tokens`. Unknown `{key}` patterns are left **verbatim** in the
/// output — prompt bodies routinely contain instructional braces like
/// `{count}` or `{{var}}` that are not template tokens and must reach
/// the model intact.
///
/// We do not support escaped braces. Anything between `{` and the next
/// `}` that does not match a known token name is copied through.
fn substitute(body: &str, tokens: &BTreeMap<&str, String>) -> String {
    let mut out = String::with_capacity(body.len());
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'{' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }
        // Find the closing brace; if missing, fall through verbatim.
        let Some(close_rel) = bytes[i + 1..].iter().position(|&b| b == b'}') else {
            out.push('{');
            i += 1;
            continue;
        };
        let close = i + 1 + close_rel;
        let key = &body[i + 1..close];
        if let Some(value) = tokens.get(key) {
            out.push_str(value);
            i = close + 1;
        } else {
            // Unknown token: emit only the opening `{` and advance past it
            // so the inner content (which may itself contain a recognised
            // token, e.g. `{{glossary_block_or_(none)}}` quoted in
            // examples) gets a normal scan.
            out.push('{');
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use i18n_harness_core::Unit;
    use i18n_harness_glossary::{Glossary, Register};
    use i18n_harness_locales::Locale;

    fn ctx_for<'a>(
        unit: &'a Unit,
        locale: &'a Locale,
        register: Register,
        glossary: Option<&'a Glossary>,
        flags: &'a i18n_harness_core::FlagSet,
    ) -> PromptContext<'a> {
        PromptContext::new(unit, locale, register, glossary, flags)
    }

    #[test]
    fn substitutes_known_tokens_and_preserves_unknown_verbatim() {
        let tpl = PromptTemplate::new(
            "Translate to {locale} ({register})\nSource: {source}\nLiteral: {count}\nDoubled: {{var}}\n",
            "v1",
        );
        let unit = Unit::untranslated_singular("x", "Hello");
        let locale = Locale::by_id("de_DE").unwrap();
        let flags = i18n_harness_core::FlagSet::new();
        let ctx = ctx_for(&unit, locale, Register::Formal, None, &flags);
        let rendered = tpl.render(&ctx);
        assert!(
            rendered.contains("Translate to de_DE (formal)"),
            "{rendered}"
        );
        assert!(rendered.contains("Source: Hello"), "{rendered}");
        // Unknown ICU-looking placeholders are preserved verbatim — prompts
        // routinely reference `{count}` as instructional text for the model.
        assert!(
            rendered.contains("Literal: {count}"),
            "{{count}} must survive verbatim: {rendered}"
        );
        assert!(
            rendered.contains("Doubled: {{var}}"),
            "{{{{var}}}} must survive verbatim: {rendered}"
        );
    }

    #[test]
    fn glossary_block_or_none_alias_substitutes_same_as_bare_name() {
        let unit = Unit::untranslated_singular("x", "Hello");
        let locale = Locale::by_id("de_DE").unwrap();
        let flags = i18n_harness_core::FlagSet::new();
        let ctx = ctx_for(&unit, locale, Register::Formal, None, &flags);
        let tpl = PromptTemplate::new(
            "[g1={glossary_block}][g2={glossary_block_or_(none)}]\n\
             [d1={do_not_translate_block}][d2={do_not_translate_block_or_(none)}]",
            "v1",
        );
        let rendered = tpl.render(&ctx);
        assert!(
            rendered.contains("[g1=(none)][g2=(none)]"),
            "both glossary aliases must substitute identically: {rendered}"
        );
        assert!(
            rendered.contains("[d1=(none)][d2=(none)]"),
            "both DNT aliases must substitute identically: {rendered}"
        );
    }

    #[test]
    fn glossary_block_uses_arrow_format() {
        let toml = r#"
[meta]
schema_version = 1

[[term]]
source = "Open"
[term.translations]
de_DE = "Öffnen"

[[term]]
source = "Save"
[term.translations]
de_DE = "Speichern"

[[term]]
source = "ChromaCheck"
do_not_translate = true
"#;
        let (g, _) = Glossary::from_toml(toml).unwrap();
        let unit = Unit::untranslated_singular("x", "Hello");
        let locale = Locale::by_id("de_DE").unwrap();
        let flags = i18n_harness_core::FlagSet::new();
        let ctx = ctx_for(&unit, locale, Register::Formal, Some(&g), &flags);
        let tpl = PromptTemplate::new("{glossary_block}\n---\n{do_not_translate_block}", "v1");
        let rendered = tpl.render(&ctx);
        assert!(
            rendered.contains("Open -> Öffnen"),
            "missing arrow line: {rendered}"
        );
        assert!(
            rendered.contains("Save -> Speichern"),
            "missing arrow line: {rendered}"
        );
        assert!(
            rendered.contains("- ChromaCheck"),
            "missing DNT bullet: {rendered}"
        );
    }

    #[test]
    fn empty_glossary_and_dnt_blocks_render_as_none() {
        let unit = Unit::untranslated_singular("x", "Hello");
        let locale = Locale::by_id("de_DE").unwrap();
        let flags = i18n_harness_core::FlagSet::new();
        let ctx = ctx_for(&unit, locale, Register::Formal, None, &flags);
        let tpl = PromptTemplate::new("g={glossary_block}|d={do_not_translate_block}", "v1");
        assert_eq!(tpl.render(&ctx), "g=(none)|d=(none)");
    }

    #[test]
    fn locale_example_block_varies_by_locale() {
        let unit = Unit::untranslated_singular("x", "Hello");
        let flags = i18n_harness_core::FlagSet::new();
        let tpl = PromptTemplate::new("[{locale_example_block_or_empty}]", "v1");

        for (id, marker) in [
            ("de_DE", "Speichern"),
            ("es_ES", "Guardar"),
            ("zh_Hans", "保存"),
        ] {
            let locale = Locale::by_id(id).unwrap();
            let ctx = ctx_for(&unit, locale, Register::Formal, None, &flags);
            let rendered = tpl.render(&ctx);
            assert!(
                rendered.contains(&format!("Examples ({id}):")),
                "missing Examples header for {id}: {rendered}"
            );
            assert!(
                rendered.contains(marker),
                "missing locale-specific term `{marker}` for {id}: {rendered}"
            );
        }

        // Unknown locale → empty block (the slot is preserved as
        // surrounding brackets with nothing between them).
        let en = Locale::by_id("en").unwrap();
        let ctx = ctx_for(&unit, en, Register::Neutral, None, &flags);
        assert_eq!(tpl.render(&ctx), "[]");
    }

    #[test]
    fn plural_category_line_is_empty_for_singular_and_set_for_plural() {
        use i18n_harness_locales::PluralCategory;
        let unit = Unit::untranslated_singular("x", "Hello");
        let locale = Locale::by_id("de_DE").unwrap();
        let flags = i18n_harness_core::FlagSet::new();
        let tpl = PromptTemplate::new("[{plural_category_line_or_empty}]", "v1");

        let ctx = ctx_for(&unit, locale, Register::Formal, None, &flags);
        assert_eq!(tpl.render(&ctx), "[]", "singular render must be empty");

        let ctx = ctx.with_plural_category(PluralCategory::One);
        let rendered = tpl.render(&ctx);
        // The directive names the category, the locale, gives a numeric
        // hint, AND tells the model not to echo source plural markers.
        assert!(
            rendered.contains("\"one\""),
            "expected category name: {rendered}"
        );
        assert!(rendered.contains("de_DE"), "expected locale: {rendered}");
        assert!(
            rendered.contains("count = 1"),
            "expected numeric hint: {rendered}"
        );
        assert!(
            rendered.contains("(s)") && rendered.contains("not literal"),
            "expected anti-marker reminder: {rendered}"
        );
    }

    #[test]
    fn template_version_is_substituted() {
        let tpl = PromptTemplate::new("[template={template_version}]\n", "v1");
        let unit = Unit::untranslated_singular("x", "Hello");
        let locale = Locale::by_id("de_DE").unwrap();
        let flags = i18n_harness_core::FlagSet::new();
        let ctx = ctx_for(&unit, locale, Register::Formal, None, &flags);
        assert_eq!(tpl.render(&ctx), "[template=v1]\n");
    }

    #[test]
    fn brace_without_close_is_preserved_verbatim() {
        let tpl = PromptTemplate::new("Hello {world without close", "v1");
        let unit = Unit::untranslated_singular("x", "Hello");
        let locale = Locale::by_id("de_DE").unwrap();
        let flags = i18n_harness_core::FlagSet::new();
        let ctx = ctx_for(&unit, locale, Register::Formal, None, &flags);
        // The "{" is preserved; everything after is literal.
        assert!(tpl.render(&ctx).starts_with("Hello {"));
    }
}
