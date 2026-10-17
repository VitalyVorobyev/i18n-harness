//! Maintainer tasks. Run with `cargo run -p xtask -- <task>`.
//!
//! `gen-locales --cldr-core <dir> --cldr-localenames <dir> [--out <file>]`
//! regenerates `crates/locales/src/cldr.rs` from the unpacked `cldr-core`
//! and `cldr-localenames-full` npm packages (CLDR JSON). The output is
//! checked in; regenerate it when moving to a new CLDR release.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;

/// Plural categories in CLDR's canonical order.
const CATEGORIES: &[(&str, &str)] = &[
    ("zero", "Zero"),
    ("one", "One"),
    ("two", "Two"),
    ("few", "Few"),
    ("many", "Many"),
    ("other", "Other"),
];

/// Integer samples at or above this are "large-number" forms (French and
/// Romance `many` for 1,000,000) that per-file plural tables such as Qt's and
/// gettext's do not give their own slot.
const SMALL_SAMPLE_LIMIT: u64 = 1_000_000;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("gen-locales") => gen_locales(&args[1..]),
        _ => bail!(
            "usage: cargo run -p xtask -- gen-locales --cldr-core <dir> \
             --cldr-localenames <dir> [--out <file>]"
        ),
    }
}

fn flag(args: &[String], name: &str) -> Option<PathBuf> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
}

fn read_json(path: &Path) -> Result<Value> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

struct Language {
    name: String,
    script: String,
    categories: Vec<&'static str>,
    small_samples: Vec<(&'static str, u64)>,
}

fn gen_locales(args: &[String]) -> Result<()> {
    let core = flag(args, "--cldr-core").ok_or_else(|| anyhow!("--cldr-core <dir> is required"))?;
    let names_dir = flag(args, "--cldr-localenames")
        .ok_or_else(|| anyhow!("--cldr-localenames <dir> is required"))?;
    let out = flag(args, "--out").unwrap_or_else(|| {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/locales/src/cldr.rs")
    });

    let core_pkg = read_json(&core.join("package.json"))?;
    let version = core_pkg["version"]
        .as_str()
        .ok_or_else(|| anyhow!("cldr-core package.json has no version"))?
        .to_owned();

    let plurals = read_json(&core.join("supplemental/plurals.json"))?;
    let plurals = plurals["supplemental"]["plurals-type-cardinal"]
        .as_object()
        .ok_or_else(|| anyhow!("plurals.json: missing plurals-type-cardinal"))?;
    let likely = read_json(&core.join("supplemental/likelySubtags.json"))?;
    let likely = likely["supplemental"]["likelySubtags"]
        .as_object()
        .ok_or_else(|| anyhow!("likelySubtags.json: missing likelySubtags"))?;
    let names = read_json(&names_dir.join("main/en/languages.json"))?;
    let names = names["main"]["en"]["localeDisplayNames"]["languages"]
        .as_object()
        .ok_or_else(|| anyhow!("languages.json: missing languages"))?;

    let mut languages: BTreeMap<String, Language> = BTreeMap::new();
    for (code, rules) in plurals {
        if code == "root" {
            continue;
        }
        let rules = rules
            .as_object()
            .ok_or_else(|| anyhow!("plural rules for {code} are not an object"))?;
        let mut categories = Vec::new();
        let mut small_samples = Vec::new();
        for (category, _) in CATEGORIES {
            let Some(rule) = rules
                .get(&format!("pluralRule-count-{category}"))
                .and_then(Value::as_str)
            else {
                continue;
            };
            categories.push(*category);
            if let Some(sample) = first_plain_integer_sample(rule)
                && sample < SMALL_SAMPLE_LIMIT
            {
                small_samples.push((*category, sample));
            }
        }
        let tag = code.replace('_', "-");
        let name = names
            .get(&tag)
            .and_then(Value::as_str)
            .unwrap_or(code)
            .to_owned();
        let script = likely
            .get(&tag)
            .or_else(|| likely.get(tag.split('-').next().unwrap_or(&tag)))
            .and_then(Value::as_str)
            .and_then(|full| full.split('-').nth(1))
            .unwrap_or("Zyyy")
            .to_owned();
        languages.insert(
            code.replace('-', "_"),
            Language {
                name,
                script,
                categories,
                small_samples,
            },
        );
    }

    let mut src = String::new();
    writeln!(
        src,
        "//! Unicode CLDR {version} data: plural categories, display names and"
    )?;
    writeln!(
        src,
        "//! likely scripts for every language with plural rules.\n//!"
    )?;
    writeln!(
        src,
        "//! Generated by `cargo run -p xtask -- gen-locales` from the `cldr-core` and"
    )?;
    writeln!(
        src,
        "//! `cldr-localenames-full` {version} packages. Do not edit by hand."
    )?;
    writeln!(
        src,
        "//! Unicode data is distributed under the Unicode License v3 (LICENSE-UNICODE).\n"
    )?;
    writeln!(src, "use crate::PluralCategory::{{self, *}};\n")?;
    writeln!(src, "/// CLDR release the table was generated from.")?;
    writeln!(src, "pub const CLDR_VERSION: &str = \"{version}\";\n")?;
    writeln!(
        src,
        "/// One CLDR language (or regional variant with its own plural rules)."
    )?;
    writeln!(src, "#[derive(Debug)]")?;
    writeln!(src, "pub(crate) struct CldrLanguage {{")?;
    writeln!(src, "    /// Tag with `_` separators (`sk`, `pt_PT`).")?;
    writeln!(src, "    pub(crate) code: &'static str,")?;
    writeln!(src, "    /// English display name.")?;
    writeln!(src, "    pub(crate) name: &'static str,")?;
    writeln!(src, "    /// ISO 15924 code of the likely script.")?;
    writeln!(src, "    pub(crate) script: &'static str,")?;
    writeln!(
        src,
        "    /// Cardinal plural categories in canonical order."
    )?;
    writeln!(src, "    pub(crate) plurals: &'static [PluralCategory],")?;
    writeln!(
        src,
        "    /// Categories with a small integer sample, with that sample."
    )?;
    writeln!(
        src,
        "    pub(crate) integer_forms: &'static [(PluralCategory, u32)],"
    )?;
    writeln!(src, "}}\n")?;
    writeln!(src, "/// Sorted by `code`.")?;
    writeln!(src, "#[rustfmt::skip]")?;
    writeln!(src, "pub(crate) static LANGUAGES: &[CldrLanguage] = &[")?;
    for (code, lang) in &languages {
        let plurals: Vec<_> = lang.categories.iter().map(|c| variant(c)).collect();
        let forms: Vec<_> = lang
            .small_samples
            .iter()
            .map(|(c, n)| format!("({}, {n})", variant(c)))
            .collect();
        writeln!(
            src,
            "    CldrLanguage {{ code: {code:?}, name: {:?}, script: {:?}, plurals: &[{}], integer_forms: &[{}] }},",
            lang.name,
            lang.script,
            plurals.join(", "),
            forms.join(", ")
        )?;
    }
    writeln!(src, "];")?;

    std::fs::write(&out, src).with_context(|| format!("write {}", out.display()))?;
    println!(
        "wrote {} languages from CLDR {version} to {}",
        languages.len(),
        out.display()
    );
    Ok(())
}

fn variant(category: &str) -> &'static str {
    CATEGORIES
        .iter()
        .find(|(name, _)| *name == category)
        .map_or("Other", |(_, v)| v)
}

/// The first `@integer` sample of a CLDR rule written as a plain number
/// (`2~4` yields 2; compact-exponent samples such as `1c6` are skipped).
fn first_plain_integer_sample(rule: &str) -> Option<u64> {
    let samples = rule.split("@integer").nth(1)?;
    let samples = samples.split('@').next().unwrap_or(samples);
    samples
        .split(',')
        .map(|s| s.trim().split('~').next().unwrap_or("").trim())
        .find_map(|s| s.parse().ok())
}
