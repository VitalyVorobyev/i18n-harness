//! Prompt profiles: how to ask one kind of model for a translation.
//!
//! A profile is data (TOML), not code: its mode (chat messages or a raw
//! completion prompt), its templates and its generation settings. The
//! built-in ones live in `crates/backend/profiles/`; which one applies to a
//! model is decided from what its server reports (see
//! [`crate::discovery::PromptStyle`]), never from the model's name.
//!
//! The model is asked for one text at a time and answers with the
//! translation only; flags come from the deterministic gate, not from the
//! model.

use std::collections::BTreeMap;

use i18n_harness_core::Unit;
use i18n_harness_glossary::{Glossary, Register};
use i18n_harness_locales::Locale;
use serde::Deserialize;

use crate::discovery::PromptStyle;

const CHAT_TOML: &str = include_str!("../profiles/chat.toml");
const COMPLETION_TOML: &str = include_str!("../profiles/completion.toml");

/// How a profile talks to the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProfileMode {
    /// System and user messages (`/api/chat`, `/v1/chat/completions`).
    Chat,
    /// One raw prompt the model continues (`/api/generate` with `raw`).
    Completion,
}

/// Generation settings sent with every request.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationOptions {
    /// Sampling temperature; 0 for reproducible output.
    #[serde(default)]
    pub temperature: f32,
    /// Random seed.
    #[serde(default)]
    pub seed: Option<u64>,
    /// Top-k sampling (1 = greedy).
    #[serde(default)]
    pub top_k: Option<u32>,
    /// Top-p sampling.
    #[serde(default)]
    pub top_p: Option<f32>,
    /// Repetition penalty.
    #[serde(default)]
    pub repeat_penalty: Option<f32>,
}

/// One prompt profile.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Short identifier (`"chat"`, `"completion"`).
    pub id: String,
    /// Chat or raw completion.
    pub mode: ProfileMode,
    /// System message (chat mode).
    #[serde(default)]
    pub system: Option<String>,
    /// The user message (chat) or the whole prompt (completion).
    pub prompt: String,
    /// Stop sequences.
    #[serde(default)]
    pub stop: Vec<String>,
    /// Stop sequences used instead of [`Self::stop`] when the text to
    /// translate is a single line.
    #[serde(default)]
    pub stop_single_line: Option<Vec<String>>,
    /// How long the server keeps the model loaded between requests.
    #[serde(default)]
    pub keep_alive: Option<String>,
    /// Generation settings.
    #[serde(default)]
    pub options: GenerationOptions,
}

/// Why a profile could not be read.
#[derive(Debug, thiserror::Error)]
#[error("invalid prompt profile: {0}")]
pub struct ProfileError(String);

impl Profile {
    /// Parse a profile from TOML.
    ///
    /// # Errors
    ///
    /// [`ProfileError`] when the TOML does not describe a profile.
    pub fn from_toml(text: &str) -> Result<Self, ProfileError> {
        toml::from_str(text).map_err(|e| ProfileError(e.to_string()))
    }

    /// The built-in profile for a prompt style ([`PromptStyle::Unknown`]
    /// gets the chat profile).
    pub fn builtin(style: PromptStyle) -> Self {
        let text = match style {
            PromptStyle::Completion => COMPLETION_TOML,
            PromptStyle::Chat | PromptStyle::Unknown => CHAT_TOML,
        };
        // The built-in files are part of the crate and covered by tests.
        Self::from_toml(text).expect("built-in prompt profiles are valid")
    }

    /// Stop sequences for a text.
    pub fn stop_for(&self, text: &str) -> &[String] {
        match (&self.stop_single_line, text.contains('\n')) {
            (Some(single), false) => single,
            _ => &self.stop,
        }
    }

    /// Render `template` with the request's values.
    pub fn render(&self, template: &str, request: &Request<'_>) -> String {
        let mut vars: BTreeMap<&str, String> = BTreeMap::new();
        vars.insert("source_language", request.source.name.to_owned());
        vars.insert("target_language", request.target.name.to_owned());
        vars.insert("text", request.unit.source.clone());
        vars.insert("notes", notes(request));
        substitute(template, &vars)
    }
}

/// One text to translate, as a profile sees it.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    /// The unit; its `source` is the text to translate, in the catalog's
    /// native placeholder syntax.
    pub unit: &'a Unit,
    /// Language of the text.
    pub source: &'a Locale,
    /// Language to translate into.
    pub target: &'a Locale,
    /// Register to use.
    pub register: Register,
    /// Project glossary.
    pub glossary: Option<&'a Glossary>,
}

/// The `{notes}` block: register, glossary, terms to keep, context. Empty
/// lines are dropped so an empty block leaves no gap.
fn notes(request: &Request<'_>) -> String {
    let mut lines = Vec::new();
    match request.register {
        Register::Formal => lines.push("Address the user formally.".to_owned()),
        Register::Informal => lines.push("Address the user informally.".to_owned()),
        Register::Neutral => {}
    }
    if let Some(glossary) = request.glossary {
        let text = &request.unit.source;
        let keep: Vec<&str> = glossary
            .do_not_translate()
            .filter(|term| text.contains(term))
            .collect();
        if !keep.is_empty() {
            lines.push(format!("Keep these terms unchanged: {}.", keep.join(", ")));
        }
        let terms: Vec<String> = glossary
            .terms_for(request.target.id)
            .chain(glossary.terms_for(request.target.language))
            .filter(|(source, _)| contains_word(text, source))
            .map(|(source, target)| format!("\"{source}\" → \"{target}\""))
            .collect();
        if !terms.is_empty() {
            lines.push(format!("Use these translations: {}.", terms.join(", ")));
        }
    }
    let context = &request.unit.context;
    if let Some(group) = context.group.as_deref().filter(|g| !g.is_empty()) {
        lines.push(format!("The text belongs to: {group}."));
    }
    if let Some(meaning) = &context.disambiguation {
        lines.push(format!("Meaning: {meaning}."));
    }
    for note in &context.developer_notes {
        lines.push(format!("Developer note: {note}"));
    }
    lines.join("\n")
}

fn contains_word(text: &str, word: &str) -> bool {
    let lower = text.to_lowercase();
    let word = word.to_lowercase();
    lower.match_indices(&word).any(|(i, _)| {
        let before = lower[..i].chars().next_back();
        let after = lower[i + word.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// Replace `{name}` for known names; leave anything else (including
/// placeholders inside the text) untouched.
fn substitute(template: &str, vars: &BTreeMap<&str, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) if vars.contains_key(&after[..end]) => {
                out.push_str(&vars[&after[..end]]);
                rest = &after[end + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    // Templates are written with readable line breaks; a blank `{notes}`
    // must not leave an empty line behind.
    out.lines()
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Tidy a model's answer: drop reasoning blocks, a repeated label, wrapping
/// quotes the original did not have, and extra lines after a single-line
/// translation. Returns `None` when nothing usable is left.
pub fn clean_output(raw: &str, input: &str, target: &Locale) -> Option<String> {
    let mut text = raw.to_owned();
    while let (Some(start), Some(end)) = (text.find("<think>"), text.find("</think>")) {
        if end < start {
            break;
        }
        text.replace_range(start..end + "</think>".len(), "");
    }
    let mut text = text.trim().to_owned();

    for label in [
        format!("{}:", target.name),
        "Translation:".to_owned(),
        "Text:".to_owned(),
    ] {
        if text
            .get(..label.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(&label))
        {
            text = text[label.len()..].trim_start().to_owned();
        }
    }

    if !input.contains('\n') && text.contains('\n') {
        text = text
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or_default()
            .to_owned();
    }

    const QUOTES: &[(&str, &str)] = &[
        ("\"", "\""),
        ("“", "”"),
        ("„", "“"),
        ("«", "»"),
        ("‘", "’"),
        ("'", "'"),
        ("⟦", "⟧"),
        ("`", "`"),
    ];
    for (open, close) in QUOTES {
        let wrapped = |s: &str| {
            s.len() >= open.len() + close.len() && s.starts_with(open) && s.ends_with(close)
        };
        if wrapped(&text) && !wrapped(input.trim()) {
            text = text[open.len()..text.len() - close.len()].trim().to_owned();
        }
    }

    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sk() -> &'static Locale {
        Locale::by_id("sk_SK").unwrap()
    }

    #[test]
    fn builtin_profiles_parse() {
        assert_eq!(Profile::builtin(PromptStyle::Chat).mode, ProfileMode::Chat);
        assert_eq!(Profile::builtin(PromptStyle::Unknown).id, "chat");
        let completion = Profile::builtin(PromptStyle::Completion);
        assert_eq!(completion.mode, ProfileMode::Completion);
        assert_eq!(completion.stop_for("one line"), ["\n"]);
        assert_eq!(completion.stop_for("two\nlines"), ["\nTranslate this from"]);
    }

    #[test]
    fn completion_prompt_is_the_models_own_format() {
        let unit = Unit::untranslated_singular("id", "Filtergröße");
        let request = Request {
            unit: &unit,
            source: Locale::by_id("de_DE").unwrap(),
            target: sk(),
            register: Register::Formal,
            glossary: None,
        };
        let profile = Profile::builtin(PromptStyle::Completion);
        assert_eq!(
            profile.render(&profile.prompt, &request),
            "Translate this from German to Slovak:\nGerman: Filtergröße\nSlovak:"
        );
    }

    #[test]
    fn chat_prompt_carries_notes_and_keeps_placeholders_in_the_text() {
        let mut unit = Unit::untranslated_singular("id", "Open ParamsManager for {name} (%1)");
        unit.context.group = Some("Import".into());
        let (glossary, _) = Glossary::from_toml(
            "[meta]\nschema_version = 1\n\n[[term]]\nsource = \"ParamsManager\"\ndo_not_translate = true\n",
        )
        .unwrap();
        let request = Request {
            unit: &unit,
            source: Locale::by_id("en").unwrap(),
            target: sk(),
            register: Register::Formal,
            glossary: Some(&glossary),
        };
        let profile = Profile::builtin(PromptStyle::Chat);
        let prompt = profile.render(&profile.prompt, &request);
        assert!(prompt.contains("Translate this English user-interface text into Slovak."));
        assert!(prompt.contains("Keep these terms unchanged: ParamsManager."));
        assert!(prompt.contains("The text belongs to: Import."));
        assert!(prompt.ends_with("Open ParamsManager for {name} (%1)"));
        assert!(!prompt.contains("\n\n"));
    }

    #[test]
    fn output_cleanup_removes_labels_quotes_reasoning_and_extra_lines() {
        let clean = |raw: &str, input: &str| clean_output(raw, input, sk());
        assert_eq!(
            clean(" Slovak: Veľkosť filtra\n", "Filter Size").as_deref(),
            Some("Veľkosť filtra")
        );
        assert_eq!(
            clean("\"Veľkosť filtra\"", "Filter Size").as_deref(),
            Some("Veľkosť filtra")
        );
        assert_eq!(
            clean(
                "<think>hmm</think>Veľkosť filtra\nNote: literal",
                "Filter Size"
            )
            .as_deref(),
            Some("Veľkosť filtra")
        );
        // Quotes the original had stay.
        assert_eq!(clean("\"Áno\"", "\"Yes\"").as_deref(), Some("\"Áno\""));
        // Multi-line originals keep their lines.
        assert_eq!(
            clean("Riadok 1\nRiadok 2", "Line 1\nLine 2").as_deref(),
            Some("Riadok 1\nRiadok 2")
        );
        assert_eq!(clean("  ", "x"), None);
        // Answers shorter than a label, with multi-byte letters, are left alone.
        assert_eq!(clean("Uložiť", "Save").as_deref(), Some("Uložiť"));
    }

    #[test]
    fn unknown_braces_in_templates_are_left_alone() {
        let unit = Unit::untranslated_singular("id", "{0} files");
        let request = Request {
            unit: &unit,
            source: Locale::by_id("en").unwrap(),
            target: sk(),
            register: Register::Neutral,
            glossary: None,
        };
        let profile = Profile::builtin(PromptStyle::Completion);
        assert!(
            profile
                .render(&profile.prompt, &request)
                .contains("English: {0} files")
        );
    }
}
