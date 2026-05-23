//! `harness` — the headless CLI entry point for `i18n-harness`.
//!
//! See `docs/initial_design.md`. The CLI grows one subcommand per milestone:
//! `round-trip` (M0) proves the byte-stability contract; `gate` (M1) runs
//! the validation gate against a target locale and optionally writes JSONL
//! metrics; `translate` (M2) runs the end-to-end loop (extract → batch →
//! backend → gate → apply). Later milestones add `export-batch` /
//! `import-batch` (M4) and `serve` (the Tauri shell, M3).

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand, ValueEnum};
use i18n_harness_adapter_qt::{apply, extract, render};
use i18n_harness_backend::{
    ManualBackend, ManualResponse, TranslatedText, TranslationBackend, TranslationOutcome,
};
use i18n_harness_core::{
    Batch, BatchKey, DEFAULT_BATCH_SIZE, Flag, FlagSeverity, Target, Unit, UnitState,
};
use i18n_harness_gate::metrics::{FileSink, MetricsWriter};
use i18n_harness_gate::{
    AccelDetail, CjkPunctuationDetail, EmptyTargetDetail, Finding, FindingDetail, GateReport,
    IcuParseDetail, LengthWarnDetail, PlaceholderAgreementDetail, PlaceholderMismatchDetail,
    PluralArityMismatchDetail, validate_batch,
};
use i18n_harness_glossary::Glossary;
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

    /// Translate a catalog end-to-end: extract → batch → backend → gate.
    /// Without `--out`, runs as dry-run (metrics only). With `--out`, writes
    /// the post-translation catalog atomically. Gate-clean units are promoted
    /// to Finished; flagged units stay Proposed.
    Translate(TranslateArgs),
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

#[derive(Parser, Debug)]
struct TranslateArgs {
    /// Path to the catalog file (Qt `.ts` for now).
    path: PathBuf,

    /// Target locale id (e.g. `de_DE`).
    #[arg(long)]
    locale: String,

    /// Which backend to use. `manual` echoes the source verbatim (useful
    /// for smoke-testing the loop without a model); `ollama` requires the
    /// CLI to be built with --features ollama.
    #[arg(long, value_enum, default_value_t = BackendChoice::Manual)]
    backend: BackendChoice,

    /// Optional glossary TOML file (loaded once, threaded into every prompt).
    #[arg(long)]
    glossary: Option<PathBuf>,

    /// If set, append one JSONL event per finding to this file.
    #[arg(long)]
    metrics: Option<PathBuf>,

    /// If set, write the post-translation catalog here atomically. Without
    /// `--out`, the run is a dry-run (translate + gate + metrics only).
    #[arg(long)]
    out: Option<PathBuf>,

    /// Batch size; defaults to the workspace constant (32).
    #[arg(long, default_value_t = DEFAULT_BATCH_SIZE)]
    batch_size: usize,
}

/// Which backend to run.
///
/// `ollama` is a `clap` value but only constructible when the CLI is built
/// with `--features ollama`; running `--backend ollama` without that feature
/// returns a configuration error at translate time.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum BackendChoice {
    /// The closure-driven `ManualBackend`. The CLI wires an identity-echo
    /// closure (returns source verbatim) so the loop is testable end-to-end
    /// without a model. The same backend powers the M4 two-phase agent path.
    Manual,
    /// Local Ollama server at `http://localhost:11434`. Requires the CLI to
    /// be built with `--features ollama` (forwards to the backend crate's
    /// matching feature).
    Ollama,
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
        Command::Translate(args) => translate(args),
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

fn translate(args: TranslateArgs) -> Result<()> {
    let locale =
        Locale::by_id(&args.locale).ok_or_else(|| anyhow!("unknown locale: {}", args.locale))?;
    if args.batch_size == 0 {
        return Err(anyhow!("--batch-size must be > 0"));
    }

    let glossary = match args.glossary.as_ref() {
        Some(p) => {
            let (g, warnings) =
                Glossary::load(p).with_context(|| format!("load glossary {}", p.display()))?;
            for w in &warnings {
                eprintln!("glossary warning: {w}");
            }
            Some(g)
        }
        None => None,
    };

    let catalog =
        extract(&args.path).with_context(|| format!("extract {}", args.path.display()))?;
    let original_units = catalog.units().to_vec();

    let backend = build_backend(args.backend)?;
    let backend_name = backend.name().to_owned();

    let writer = args
        .metrics
        .as_ref()
        .map(|p| MetricsWriter::new(&backend_name, &args.locale, FileSink::new(p)));

    // Partition: keep writable units (Untranslated/Proposed) for translation;
    // everything else is preserved as-is and not passed to the backend.
    let mut writable: Vec<Unit> = Vec::new();
    let mut preserved: Vec<Unit> = Vec::new();
    for u in &original_units {
        if u.state.is_writable() {
            writable.push(u.clone());
        } else {
            preserved.push(u.clone());
        }
    }

    let mut translated_units: Vec<Unit> = Vec::with_capacity(writable.len());
    let mut summary = TranslateSummary::default();

    let file_hash = file_hash(&args.path)?;
    for (idx, chunk) in writable.chunks(args.batch_size).enumerate() {
        let batch = Batch::new(BatchKey::new(&file_hash, idx as u32), chunk.to_vec());
        let outcomes = backend
            .translate_batch(&batch, locale, glossary.as_ref())
            .with_context(|| format!("batch {idx} translate"))?;

        if outcomes.len() != batch.units.len() {
            return Err(anyhow!(
                "backend `{backend_name}` returned {got} outcomes for batch of {expected}",
                got = outcomes.len(),
                expected = batch.units.len(),
            ));
        }

        let merged: Vec<Unit> = batch
            .units
            .iter()
            .zip(outcomes.iter())
            .map(|(unit, outcome)| merge_outcome(unit, outcome, &mut summary))
            .collect();

        // Gate validates everything we attempted (even Skipped / Failed
        // outcomes which kept the unit untranslated — those should not
        // produce gate findings since target is None/empty per its state).
        let reports = validate_batch(&merged, locale, glossary.as_ref());

        for (unit, report) in merged.into_iter().zip(reports) {
            if let Some(w) = writer.as_ref()
                && !report.findings.is_empty()
                && let Err(e) = w.record_report(&report)
            {
                eprintln!("warning: failed to write metrics line: {e}");
            }

            let mut promoted = unit;
            if !report.has_hard() && promoted.target.is_complete() {
                // Gate-clean and target present: promote to Finished. Soft
                // findings are not blocking.
                promoted.state = UnitState::Finished;
                summary.finished += 1;
            } else if !report.is_clean() {
                summary.flagged += 1;
                for finding in &report.findings {
                    match finding.flag.severity() {
                        FlagSeverity::Hard => summary.hard += 1,
                        FlagSeverity::Soft => summary.soft += 1,
                        FlagSeverity::Semantic => {}
                    }
                }
                print_unit(&promoted, &report);
            }

            translated_units.push(promoted);
        }
    }

    // Re-assemble the override slice: translated units (with their final
    // state) plus preserved non-writable units (unchanged).
    let mut overrides = translated_units;
    overrides.extend(preserved);

    // Always render to check the bytes are well-formed; only write if --out.
    let bytes =
        render(&catalog, &overrides).with_context(|| format!("render {}", args.path.display()))?;
    let _ = bytes; // consumed for validation; written below if --out is set.

    if let Some(out_path) = args.out.as_ref() {
        apply(&catalog, &overrides, out_path)
            .with_context(|| format!("apply → {}", out_path.display()))?;
        println!(
            "\nwrote: {} ({} units)",
            out_path.display(),
            overrides.len()
        );
    } else {
        println!("\ndry-run (no --out); skipped write-back");
    }

    println!(
        "translate report: {} → locale={} backend={}  writable={} translated={} skipped={} failed={} finished={} flagged={} hard={} soft={}",
        args.path.display(),
        args.locale,
        backend_name,
        summary.writable_total,
        summary.translated,
        summary.skipped,
        summary.failed,
        summary.finished,
        summary.flagged,
        summary.hard,
        summary.soft,
    );

    if summary.hard > 0 {
        return Err(anyhow!(
            "{} hard finding(s) — write-back blocked for affected units",
            summary.hard
        ));
    }
    Ok(())
}

#[derive(Debug, Default)]
struct TranslateSummary {
    writable_total: usize,
    translated: usize,
    skipped: usize,
    failed: usize,
    finished: usize,
    flagged: usize,
    hard: usize,
    soft: usize,
}

fn build_backend(choice: BackendChoice) -> Result<Box<dyn TranslationBackend>> {
    match choice {
        BackendChoice::Manual => Ok(Box::new(ManualBackend::new(echo_response))),
        BackendChoice::Ollama => {
            #[cfg(feature = "ollama")]
            {
                Ok(Box::new(i18n_harness_backend::ollama::OllamaBackend::new()?))
            }
            #[cfg(not(feature = "ollama"))]
            {
                Err(anyhow!(
                    "ollama backend not compiled in; rebuild with `--features ollama`"
                ))
            }
        }
    }
}

/// Identity-echo closure for `--backend manual`. Returns the source verbatim
/// for singular units; for plural units it broadcasts the source across every
/// CLDR slot. The result will likely fire gate length-warn or accelerator
/// findings — that is the point: it lets the operator see the whole pipeline
/// run before they have a real model installed.
fn echo_response(ctx: &i18n_harness_backend::PromptContext<'_>) -> ManualResponse {
    let source = ctx.unit.source.clone();
    match ctx.unit.plural_arity {
        None => ManualResponse::Singular(source),
        Some(arity) => ManualResponse::Plural(vec![source; arity as usize]),
    }
}

fn merge_outcome(
    unit: &Unit,
    outcome: &TranslationOutcome,
    summary: &mut TranslateSummary,
) -> Unit {
    summary.writable_total += 1;
    let mut out = unit.clone();
    match outcome {
        TranslationOutcome::Translated { text, flags } => {
            summary.translated += 1;
            out.target = match text {
                TranslatedText::Singular(s) => Target::Singular {
                    text: Some(s.clone()),
                },
                TranslatedText::Plural(forms) => Target::Plural {
                    forms: forms.iter().cloned().map(Some).collect(),
                },
            };
            // Promote intermediate state so the gate's
            // EmptyTargetWhenFinished does not fire on the freshly-merged
            // unit while we are still deciding whether to promote to
            // Finished after the gate runs.
            out.state = UnitState::Proposed;
            for flag in flags {
                out.flags.insert(*flag);
            }
        }
        TranslationOutcome::Skipped { .. } => {
            summary.skipped += 1;
            // Leave the unit untranslated.
        }
        TranslationOutcome::Failed { .. } => {
            summary.failed += 1;
            // Leave the unit untranslated.
        }
    }
    out
}

fn file_hash(path: &std::path::Path) -> Result<String> {
    use std::hash::{Hash, Hasher};
    let bytes = std::fs::read(path).with_context(|| format!("read {} for hash", path.display()))?;
    // Non-cryptographic; enough to disambiguate batches of the same file
    // across runs. A real content hash (SHA-256) lands when we wire the
    // resumable-batch persistence; the resume key is a string slot per the
    // core::Batch contract, so swapping in is a one-line change.
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    Ok(format!("{:016x}", h.finish()))
}
