//! `harness import-glossary`: merge a term table (as a spreadsheet saves
//! it) into a folder's glossary.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use i18n_harness_session::Session;

#[derive(Parser, Debug)]
pub(crate) struct ImportGlossaryArgs {
    /// Folder with the catalogs. Opening it creates its settings file when
    /// there is none; the glossary is created next to it when missing.
    folder: PathBuf,
    /// Term table: rows of `term, translation, note`, comma, semicolon or
    /// tab separated. A row without a translation keeps the term unchanged.
    table: PathBuf,
    /// Language the translations are in (e.g. `sk_SK`).
    #[arg(long)]
    language: String,
}

pub(crate) fn import_glossary_table(args: ImportGlossaryArgs) -> Result<()> {
    let table = std::fs::read_to_string(&args.table)
        .with_context(|| format!("read {}", args.table.display()))?;
    let mut session = Session::open_folder(&args.folder)
        .with_context(|| format!("open {}", args.folder.display()))?;
    let summary = session
        .import_glossary(&args.language, &table)
        .with_context(|| format!("import {}", args.table.display()))?;
    let entries = session.glossary(&args.language);
    let keep = entries.iter().filter(|e| e.keep).count();
    eprintln!(
        "{} term(s) added, {} updated; the glossary now has {} term(s), {keep} kept unchanged",
        summary.added,
        summary.updated,
        entries.len()
    );
    eprintln!(
        "apply it to strings translated before: harness translate-folder {} --fix-glossary …",
        args.folder.display()
    );
    Ok(())
}
