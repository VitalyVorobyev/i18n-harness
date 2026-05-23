//! `harness` — the headless CLI entry point for `i18n-harness`.
//!
//! See `docs/initial_design.md`. M0 ships one subcommand: `round-trip`,
//! which proves the byte-stability contract end-to-end for a Qt `.ts`
//! catalog. Other subcommands (`extract`, `apply`, `translate`,
//! `export-batch`, `import-batch`) land milestone by milestone.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use i18n_harness_adapter_qt::{extract, render};

#[derive(Parser, Debug)]
#[command(
    name = "harness",
    version,
    about = "i18n-harness CLI",
    long_about = "Local-first translation harness. See docs/initial_design.md for the design \
                  intent. Subcommands land milestone by milestone."
)]
struct Cli {
    /// Subcommand to run.
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Verify the byte-stable round-trip contract on a Qt `.ts` file:
    /// extract → apply with zero unit changes must produce byte-identical
    /// output. Exits non-zero on any diff.
    RoundTrip(RoundTripArgs),
}

#[derive(Parser, Debug)]
struct RoundTripArgs {
    /// Path to the Qt `.ts` file to round-trip.
    path: PathBuf,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::RoundTrip(args) => round_trip(args),
    }
}

fn round_trip(args: RoundTripArgs) -> Result<()> {
    let catalog =
        extract(&args.path).with_context(|| format!("extract {}", args.path.display()))?;
    let rendered =
        render(&catalog, &[]).with_context(|| format!("render {}", args.path.display()))?;
    let original = catalog.source_bytes();

    if rendered == original {
        println!(
            "round-trip OK: {} ({} bytes, {} units)",
            args.path.display(),
            original.len(),
            catalog.units().len(),
        );
        return Ok(());
    }

    // Byte-diverge report. Show the first divergence in the input, the
    // line+column, and the surrounding context.
    let common = original
        .iter()
        .zip(rendered.iter())
        .take_while(|(x, y)| x == y)
        .count();
    let line = original[..common].iter().filter(|&&b| b == b'\n').count() + 1;
    let col = common
        - original[..common]
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |p| p + 1);
    let snippet_len = 80usize;
    let orig_snip =
        String::from_utf8_lossy(&original[common..original.len().min(common + snippet_len)])
            .into_owned();
    let new_snip =
        String::from_utf8_lossy(&rendered[common..rendered.len().min(common + snippet_len)])
            .into_owned();

    eprintln!(
        "round-trip FAILED for {}\n  diverge at byte {common} (line {line}, col {col})\n  original len: {}\n  rendered len: {}\n  original: {:?}\n  rendered: {:?}",
        args.path.display(),
        original.len(),
        rendered.len(),
        orig_snip,
        new_snip,
    );
    Err(anyhow!("byte-stable round-trip contract violated"))
}
