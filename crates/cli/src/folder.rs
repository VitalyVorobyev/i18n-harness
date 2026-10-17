//! `harness translate-folder`: translate every string still to do in a
//! folder of catalogs, unattended, through the same session the desktop app
//! uses.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};
use i18n_harness_session::{
    CancelToken, InputText, IssueSeverity, ModelSettings, Progress, Promotion, ServerKind, Session,
    TranslateScope, UnitFilter, UnitStatus,
};

#[derive(Parser, Debug)]
pub(crate) struct TranslateFolderArgs {
    /// Folder with the catalogs. Opening it creates its settings file
    /// (`i18n-harness.toml`) when there is none.
    folder: PathBuf,
    /// Model to translate with, as the server names it.
    #[arg(long)]
    model: String,
    /// Language the catalogs' existing texts are in, when it is not the
    /// catalogs' own language (e.g. `de_DE` for Slovak files pre-filled
    /// with German). Recorded in the settings file.
    #[arg(long)]
    pivot: Option<String>,
    /// What to translate from.
    #[arg(long, value_enum, default_value_t = Input::Auto)]
    input: Input,
    /// Kind of model server.
    #[arg(long, value_enum, default_value_t = Server::Ollama)]
    server: Server,
    /// Server address.
    #[arg(long, default_value = i18n_harness_session::DEFAULT_OLLAMA_HOST)]
    host: String,
    /// Mark clean translations as done (finished in the file) instead of
    /// leaving them to check. For output that is delivered as is.
    #[arg(long)]
    deliver: bool,
    /// Write the catalogs when the run ends.
    #[arg(long)]
    save: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Input {
    /// The existing text when the catalog has a pivot language and that text
    /// is sound, else the original.
    Auto,
    /// Always the original text.
    Source,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Server {
    Ollama,
    OpenaiCompatible,
}

pub(crate) fn translate_folder(args: TranslateFolderArgs) -> Result<()> {
    let mut session = Session::open_folder(&args.folder)
        .with_context(|| format!("open {}", args.folder.display()))?;
    for warning in session.warnings() {
        eprintln!("warning: {warning}");
    }
    if let Some(pivot) = &args.pivot {
        let paths: Vec<PathBuf> = session
            .catalogs()
            .into_iter()
            .map(|c| c.path.into())
            .collect();
        for path in paths {
            session.set_pivot(&path, Some(pivot))?;
        }
    }

    let settings = ModelSettings {
        server: match args.server {
            Server::Ollama => ServerKind::Ollama,
            Server::OpenaiCompatible => ServerKind::OpenAiCompatible,
        },
        host: args.host.clone(),
        model: Some(args.model.clone()),
        num_ctx: None,
    };
    let backend = i18n_harness_session::backend(&settings)?;
    let input = match args.input {
        Input::Auto => InputText::Auto,
        Input::Source => InputText::Source,
    };
    let promotion = if args.deliver {
        Promotion::Deliver
    } else {
        Promotion::Review
    };

    let started = Instant::now();
    let mut report = |p: Progress| match p {
        Progress::Started { total } => eprintln!("translating {total} strings with {}", args.model),
        Progress::Unit {
            done,
            total,
            failure,
            id,
            ..
        } => {
            if let Some(reason) = failure {
                eprintln!("  failed: {id}: {reason}");
            }
            if done % 100 == 0 || done == total {
                let rate = done as f64 / started.elapsed().as_secs_f64().max(0.001);
                eprintln!("  {done}/{total} ({rate:.1} strings/s)");
            }
        }
        Progress::Finished {
            translated,
            failed,
            cancelled,
        } => eprintln!(
            "{} {translated} translated, {failed} failed in {:.0} s",
            if cancelled { "stopped:" } else { "done:" },
            started.elapsed().as_secs_f64()
        ),
    };
    session.translate(
        &TranslateScope::default(),
        input,
        promotion,
        backend.as_ref(),
        &mut report,
        &CancelToken::new(),
    )?;

    print_problems(&session)?;

    if args.save {
        let saved = session.save(None)?;
        eprintln!(
            "saved {} file(s){}",
            saved.written.len(),
            if saved.held_back > 0 {
                format!(
                    "; {} translation(s) with errors kept their previous text",
                    saved.held_back
                )
            } else {
                String::new()
            }
        );
    } else {
        eprintln!("not saved (pass --save to write the catalogs)");
    }
    for c in session.catalogs() {
        println!(
            "{}: {} to do, {} to check, {} done, {} with errors",
            c.relative_path, c.counts.to_do, c.counts.check, c.counts.done, c.counts.errors
        );
    }
    Ok(())
}

/// List every string whose translation has an error, so it can be fixed by
/// hand.
fn print_problems(session: &Session) -> Result<()> {
    let filter = UnitFilter {
        statuses: vec![UnitStatus::Check, UnitStatus::Done],
        errors_only: true,
        search: None,
    };
    let mut any = false;
    for c in session.catalogs() {
        let page = session.units(Path::new(&c.path), &filter, 0, usize::MAX)?;
        for unit in page.units {
            any = true;
            println!("{}: {}", c.relative_path, unit.source.replace('\n', "⏎"));
            println!("  → {}", unit.target.join(" | ").replace('\n', "⏎"));
            for issue in unit
                .issues
                .iter()
                .filter(|i| i.severity == IssueSeverity::Error)
            {
                println!("  ! {}", issue.message);
            }
        }
    }
    if !any {
        eprintln!("no translation has an error");
    }
    if session.catalogs().iter().any(|c| c.counts.errors > 0) && !any {
        bail!("inconsistent error counts");
    }
    Ok(())
}
