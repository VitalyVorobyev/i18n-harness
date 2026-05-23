//! `harness` — the headless CLI entry point for `i18n-harness`.
//!
//! See `docs/initial_design.md`. The CLI grows one subcommand per milestone:
//! `round-trip` (M0) proves the byte-stability contract; `gate` (M1) runs
//! the validation gate against a target locale and optionally writes JSONL
//! metrics. Later milestones add `translate`, `export-batch`, `import-batch`.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use i18n_harness_adapter_qt::{extract, render};
use i18n_harness_core::{Flag, FlagSeverity};
use i18n_harness_gate::metrics::{FileSink, MetricsWriter};
use i18n_harness_gate::{
    AccelDetail, CjkPunctuationDetail, EmptyTargetDetail, Finding, FindingDetail, GateReport,
    IcuParseDetail, LengthWarnDetail, PlaceholderAgreementDetail, PlaceholderMismatchDetail,
    PluralArityMismatchDetail, validate_batch,
};
use i18n_harness_locales::Locale;

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

    /// Run the validation gate against a target locale. Prints one section
    /// per unit with findings; exits non-zero if any hard flag fires.
    Gate(GateArgs),
}

#[derive(Parser, Debug)]
struct RoundTripArgs {
    /// Path to the Qt `.ts` file to round-trip.
    path: PathBuf,
}

#[derive(Parser, Debug)]
struct GateArgs {
    /// Path to the catalog file (Qt `.ts` for now).
    path: PathBuf,

    /// Target locale id (e.g. `de_DE`).
    #[arg(long)]
    locale: String,

    /// Backend label recorded in metrics events. Defaults to `manual` for
    /// gate-only invocations.
    #[arg(long, default_value = "manual")]
    backend: String,

    /// If set, append one JSONL event per finding to this file.
    #[arg(long)]
    metrics: Option<PathBuf>,
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
        Command::Gate(args) => gate(args),
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

fn gate(args: GateArgs) -> Result<()> {
    let locale =
        Locale::by_id(&args.locale).ok_or_else(|| anyhow!("unknown locale: {}", args.locale))?;
    let catalog =
        extract(&args.path).with_context(|| format!("extract {}", args.path.display()))?;
    let units = catalog.units();
    let reports = validate_batch(units, locale, None);

    let writer = args
        .metrics
        .as_ref()
        .map(|p| MetricsWriter::new(&args.backend, &args.locale, FileSink::new(p)));

    let mut hard = 0usize;
    let mut soft = 0usize;
    let mut clean = 0usize;

    for (unit, report) in units.iter().zip(reports.iter()) {
        if report.is_clean() {
            clean += 1;
            continue;
        }
        if let Some(w) = writer.as_ref() {
            if let Err(e) = w.record_report(report) {
                eprintln!("warning: failed to write metrics line: {e}");
            }
        }
        for finding in &report.findings {
            match finding.flag.severity() {
                FlagSeverity::Hard => hard += 1,
                FlagSeverity::Soft => soft += 1,
                FlagSeverity::Semantic => {}
            }
        }
        print_unit(unit, report);
    }

    println!(
        "\ngate report: {}  locale={}  units={}  hard={}  soft={}  clean={}",
        args.path.display(),
        args.locale,
        units.len(),
        hard,
        soft,
        clean,
    );

    if hard > 0 {
        return Err(anyhow!(
            "{hard} hard finding(s) — write-back blocked for {}",
            args.path.display()
        ));
    }
    Ok(())
}

fn print_unit(unit: &i18n_harness_core::Unit, report: &GateReport) {
    let header = match (unit.provenance.file.as_str(), unit.provenance.line) {
        ("", _) => format!("{}", unit.id),
        (file, Some(line)) => format!("{file}:{line}  {}", unit.id),
        (file, None) => format!("{file}  {}", unit.id),
    };
    println!("\n{header}");
    for finding in &report.findings {
        let sev = match finding.flag.severity() {
            FlagSeverity::Hard => "HARD",
            FlagSeverity::Soft => "soft",
            FlagSeverity::Semantic => "info",
        };
        println!(
            "  [{sev}] {}: {}",
            flag_name(finding.flag),
            render_detail(finding)
        );
    }
}

fn render_detail(f: &Finding) -> String {
    match &f.detail {
        FindingDetail::PlaceholderMismatch(PlaceholderMismatchDetail {
            slot,
            missing,
            extra,
        }) => format!(
            "slot {slot}: missing={missing:?} extra={extra:?}",
            missing = missing,
            extra = extra,
        ),
        FindingDetail::PluralArityMismatch(PluralArityMismatchDetail {
            expected,
            found,
            wrong_variant,
        }) => {
            if *wrong_variant {
                format!("expected {expected} plural forms, got singular target")
            } else {
                format!("expected {expected} plural forms, got {found}")
            }
        }
        FindingDetail::IcuParseError(IcuParseDetail {
            slot,
            byte_offset,
            message,
        }) => format!("slot {slot} at byte {byte_offset}: {message}"),
        FindingDetail::EmptyTargetWhenFinished(EmptyTargetDetail { slot }) => {
            format!("slot {slot} is empty but unit state is Finished")
        }
        FindingDetail::AccelMismatch(AccelDetail {
            source_count,
            target_count,
        }) => format!("source has {source_count} '&', target has {target_count}"),
        FindingDetail::LengthWarn(LengthWarnDetail {
            source_chars,
            target_chars,
            threshold,
            ratio,
        }) => format!(
            "{target_chars} chars vs source {source_chars} (ratio {ratio:.2}, threshold {threshold:.2})"
        ),
        FindingDetail::CjkPunctuationTolerated(CjkPunctuationDetail { characters }) => {
            let s: String = characters.iter().collect();
            format!("full-width punctuation in target: {s}")
        }
        FindingDetail::PlaceholderAgreementRisk(PlaceholderAgreementDetail {
            placeholder,
            determiner,
        }) => format!("{determiner} {placeholder} — gender/case unresolved at translation time"),
    }
}

fn flag_name(flag: Flag) -> &'static str {
    // Kept in step with `Flag`'s serde `rename_all = "kebab-case"`. Adding a
    // variant trips the non-exhaustive match — the intended trip-wire.
    match flag {
        Flag::PlaceholderMismatch => "placeholder-mismatch",
        Flag::PluralArityMismatch => "plural-arity-mismatch",
        Flag::IcuParseError => "icu-parse-error",
        Flag::EmptyTargetWhenFinished => "empty-target-when-finished",
        Flag::AccelMismatch => "accel-mismatch",
        Flag::LengthWarn => "length-warn",
        Flag::CjkPunctuationTolerated => "cjk-punctuation-tolerated",
        Flag::PlaceholderAgreementRisk => "placeholder-agreement-risk",
        Flag::AmbiguousSource => "ambiguous-source",
        Flag::Idiom => "idiom",
        Flag::InsufficientContext => "insufficient-context",
        Flag::LowConfidence => "low-confidence",
    }
}
