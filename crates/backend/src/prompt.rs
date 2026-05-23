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
//! | `{glossary_block}` | A list of glossary entries for the target locale, one `<src> -> <tgt>` per line, or `"(none)"` if empty. |
//! | `{do_not_translate_block}` | A bullet list of DNT sources, or `"(none)"` if empty. |
//! | `{template_version}` | The template's declared version (string, e.g. `"v1"`). |
//!
//! Unknown tokens in the template are left as literal text (with a
//! leading `?` to make them grep-able), not silently dropped — a typo in
//! a token name should be obvious in the rendered prompt.

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
        tokens.insert("glossary_block", render_glossary_block(ctx));
        tokens.insert("do_not_translate_block", render_do_not_translate_block(ctx));
        tokens.insert("template_version", self.version.clone());
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

/// Walk `body` and replace `{key}` occurrences with their values from
/// `tokens`. Unknown tokens are left as literal `{?key}` so a typo is
/// obvious in the rendered output.
///
/// We do not support escaped braces (`{{`/`}}`) because the v1 template
/// vocabulary has no use for literal braces in the prompt envelope. If
/// that changes, this function gains an escape pass; until then, keeping
/// it simple matches the surface area.
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
        } else {
            out.push('{');
            out.push('?');
            out.push_str(key);
            out.push('}');
        }
        i = close + 1;
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
    fn substitutes_known_tokens_and_marks_unknown() {
        let tpl = PromptTemplate::new(
            "Translate to {locale} ({register})\nSource: {source}\nBogus: {nope}\n",
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
        // Unknown token preserved with `?` marker so a typo is visible.
        assert!(rendered.contains("{?nope}"), "{rendered}");
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
