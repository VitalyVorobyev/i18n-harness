//! `harness` — the headless CLI entry point for `i18n-harness`.
//!
//! See `docs/initial_design.md`. Subcommands:
//! - `round-trip` — proves the byte-stability contract for a catalog.
//! - `gate` — runs the validation gate against a target locale and
//!   optionally writes JSONL metrics.
//! - `translate` — runs the end-to-end loop (extract → batch → backend
//!   → gate → apply).
//! - `init` / `open` — discover and load i18n-harness projects.
//! - `export-batch` / `import-batch` — the two-phase agent translation
//!   flow; `export-batch` writes a batch folder an external agent
//!   (Claude Code / Copilot / Codex) fills, `import-batch` ingests the
//!   result through the manual backend.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand, ValueEnum};
use i18n_harness_adapter_qt::{apply, extract, render, write_subset};
use i18n_harness_backend::{
    ManualBackend, ManualResponse, TranslatedText, TranslationBackend, TranslationOutcome,
    agent_batch,
};
use i18n_harness_core::{
    Batch, BatchKey, DEFAULT_BATCH_SIZE, Flag, FlagSeverity, Target, Unit, UnitState,
};
use i18n_harness_gate::metrics::{FileSink, MetricsWriter};
use i18n_harness_gate::{
    AccelDetail, CjkPunctuationDetail, EmptyTargetDetail, Finding, FindingDetail, GateReport,
    IcuParseDetail, LengthWarnDetail, MarkupTagMismatchDetail, PlaceholderAgreementDetail,
    PlaceholderMismatchDetail, PluralArityMismatchDetail, validate_batch,
};
use i18n_harness_glossary::Glossary;
use i18n_harness_locales::Locale;
use i18n_harness_project::Project;
use i18n_harness_reuse::{
    ReuseError, merge_back, reuse_from_references, writable_untranslated_ids,
};

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

    /// Discover catalogs under `<dir>`, build a draft manifest, and write it
    /// to `<dir>/i18n-harness.toml`. Refuses to overwrite an existing manifest
    /// unless `--force` is passed.
    Init(InitArgs),

    /// Open the project at `<dir>` (loads and validates
    /// `<dir>/i18n-harness.toml`) and print its summary. Exits non-zero if
    /// the manifest is missing or invalid.
    Open(OpenArgs),

    /// Phase 1 of the two-phase agent flow: extract writable units from a
    /// catalog, build a prompt, and write the export folder an external agent
    /// fills in. Prints the absolute path of the output directory on stdout.
    /// Fully deterministic — no model, no network.
    ExportBatch(ExportBatchArgs),

    /// Phase 2 of the two-phase agent flow: read the agent-filled
    /// `targets.jsonl` from an export folder, run the gate, and optionally
    /// write the post-translation catalog. Without `--out`, runs as dry-run.
    ImportBatch(ImportBatchArgs),

    /// Copy expert translations from one or more reference catalogs into a
    /// base catalog by exact unit-id match. Agreed references are applied;
    /// conflicts are reported but left for a human to resolve. Writes the
    /// result back in place unless `--out` is given (ad-hoc single-base mode).
    Reuse(ReuseArgs),

    /// Extract the writable-untranslated unit ids from a base catalog and
    /// write them as a standalone Qt `.ts` remainder file. Run this right
    /// after `reuse` to get the post-reuse leftover that needs translation.
    SplitRemainder(SplitRemainderArgs),

    /// Fold a translated remainder back into its base catalog. The remainder
    /// must be a subset of the base (ids present in base); ids that are
    /// finished-and-complete in both files are treated as an overlap error.
    Merge(MergeArgs),
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
struct InitArgs {
    /// Directory to scan and write a manifest into.
    dir: PathBuf,

    /// Overwrite an existing `i18n-harness.toml` if present.
    #[arg(long)]
    force: bool,
}

#[derive(Parser, Debug)]
struct OpenArgs {
    /// Project root containing `i18n-harness.toml`.
    dir: PathBuf,
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

#[derive(Parser, Debug)]
struct ExportBatchArgs {
    /// Single-catalog mode: path to the Qt `.ts` catalog to export.
    ///
    /// Mutually exclusive with `--project`. Only Qt catalogs are supported in
    /// this release; non-Qt catalogs in a project are warned about and skipped.
    #[arg(group = "source", required_unless_present = "project")]
    path: Option<PathBuf>,

    /// Project mode: project root directory containing `i18n-harness.toml`.
    ///
    /// Mutually exclusive with the positional `<PATH>` argument.
    #[arg(long, group = "source")]
    project: Option<PathBuf>,

    /// Target locale id (e.g. `de_DE`).
    #[arg(long)]
    locale: String,

    /// Directory to write the export folder into. Created if absent.
    ///
    /// In project mode, each matching catalog gets its own subfolder.
    #[arg(long)]
    out: PathBuf,

    /// Single-catalog mode only: optional glossary TOML file. Terms are
    /// inlined into `prompt.md`; load failures abort. Ignored in project
    /// mode — the manifest glossary is used instead.
    #[arg(long, conflicts_with = "project")]
    glossary: Option<PathBuf>,
}

#[derive(Parser, Debug)]
struct ImportBatchArgs {
    /// Export folder produced by `export-batch` (contains `units.jsonl`,
    /// `targets.jsonl`, etc., or subfolders in project mode).
    dir: PathBuf,

    /// Single-catalog mode: path to the catalog the batch was extracted from.
    ///
    /// Mutually exclusive with `--project`.
    #[arg(long, group = "target")]
    apply: Option<PathBuf>,

    /// Project mode: project root directory containing `i18n-harness.toml`.
    ///
    /// Mutually exclusive with `--apply`.
    #[arg(long, group = "target")]
    project: Option<PathBuf>,

    /// If set, append one JSONL event per finding to this file.
    #[arg(long)]
    metrics: Option<PathBuf>,

    /// Single-catalog mode only: write the post-translation catalog here
    /// atomically. Without `--out`, runs as dry-run.
    #[arg(long, conflicts_with = "project")]
    out: Option<PathBuf>,

    /// Project mode only: write each post-translation catalog under this
    /// directory, preserving the manifest-relative directory structure.
    /// Without `--out-dir`, runs as dry-run.
    #[arg(long, conflicts_with = "apply")]
    out_dir: Option<PathBuf>,
}

#[derive(Parser, Debug)]
struct ReuseArgs {
    /// Ad-hoc mode: base Qt `.ts` catalog to reuse translations into.
    ///
    /// Mutually exclusive with `--project`. The file is read; output goes to
    /// `--out` if given, otherwise the base is overwritten in place.
    #[arg(group = "source", required_unless_present = "project")]
    path: Option<PathBuf>,

    /// Project mode: project root directory containing `i18n-harness.toml`.
    ///
    /// Mutually exclusive with the positional `<PATH>` argument. The project
    /// manifest determines both the base catalogs (filtered to `--locale`) and
    /// the reference set for that locale; each matching base is written in
    /// place. Non-Qt catalog entries are warned about and skipped.
    #[arg(long, group = "source")]
    project: Option<PathBuf>,

    /// Target locale id (e.g. `de_DE`). Required in both modes.
    #[arg(long)]
    locale: String,

    /// Ad-hoc mode only: one or more reference Qt `.ts` catalogs to pull
    /// translations from. Repeated: `--reference a.ts --reference b.ts`.
    /// Required in ad-hoc mode; ignored in project mode (the manifest
    /// `[[references]]` for the locale are used instead).
    #[arg(long = "reference", conflicts_with = "project")]
    references: Vec<PathBuf>,

    /// Ad-hoc single-base mode only: write the result here instead of
    /// overwriting the base in place. Ignored in project mode.
    #[arg(long, conflicts_with = "project")]
    out: Option<PathBuf>,

    /// Ad-hoc mode only: optional glossary TOML file. Threaded into the gate
    /// so glossary-aware checks run on copied translations. Ignored in project
    /// mode — the manifest glossary is used instead.
    #[arg(long, conflicts_with = "project")]
    glossary: Option<PathBuf>,
}

#[derive(Parser, Debug)]
struct SplitRemainderArgs {
    /// Base Qt `.ts` catalog to split. Writable-untranslated units are written
    /// to `--out`; the base itself is not modified.
    ///
    /// Running this right after `reuse` gives the post-reuse leftover whose
    /// units still need translation.
    base: PathBuf,

    /// Write the remainder catalog here.
    #[arg(long)]
    out: PathBuf,
}

#[derive(Parser, Debug)]
struct MergeArgs {
    /// Base Qt `.ts` catalog (the half that was not sent for translation).
    base: PathBuf,

    /// Translated remainder Qt `.ts` produced by a translator from the
    /// split step. Its unit ids must be a subset of `<BASE>`'s ids and
    /// disjoint from the base's finished-and-complete units.
    #[arg(long = "with")]
    remainder: PathBuf,

    /// Write the merged catalog here (base + remainder translations applied).
    #[arg(long)]
    out: PathBuf,
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
    /// without a model. The same backend powers the two-phase agent CLI
    /// (`harness import-batch`).
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
        Command::Init(args) => init(args),
        Command::Open(args) => open(args),
        Command::ExportBatch(args) => export_batch(args),
        Command::ImportBatch(args) => import_batch(args),
        Command::Reuse(args) => reuse(args),
        Command::SplitRemainder(args) => split_remainder(args),
        Command::Merge(args) => merge(args),
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
        FindingDetail::MarkupTagMismatch(MarkupTagMismatchDetail {
            slot,
            missing,
            extra,
        }) => format!("slot {slot}: missing tags={missing:?} extra tags={extra:?}"),
        FindingDetail::BackendMalformedResponse(detail) => {
            format!("backend response could not be parsed: {}", detail.reason)
        }
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
        Flag::MarkupTagMismatch => "markup-tag-mismatch",
        Flag::BackendMalformedResponse => "backend-malformed-response",
        Flag::BrandTerm => "brand-term",
        Flag::ToneMismatch => "tone-mismatch",
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
            if report.is_clean() && promoted.target.is_complete() {
                // Fully gate-clean (no hard AND no soft findings) and target
                // present: safe to promote to Finished. Any finding — hard or
                // soft — keeps the unit at Proposed for human review.
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

    // Always render to validate the bytes are well-formed, regardless of
    // findings; renders are pure and surface adapter bugs without mutating
    // disk state.
    let _bytes =
        render(&catalog, &overrides).with_context(|| format!("render {}", args.path.display()))?;

    // Hard-finding guard MUST precede `apply()` — write-back is blocked any
    // time the gate found a hard issue, even in --out mode. Soft-only runs
    // still write; the affected units stay at Proposed.
    let write_blocked = summary.hard > 0;

    if let Some(out_path) = args.out.as_ref() {
        if write_blocked {
            eprintln!(
                "\nblocked: {} hard finding(s); not writing {} (use `harness gate` to inspect)",
                summary.hard,
                out_path.display(),
            );
        } else {
            apply(&catalog, &overrides, out_path)
                .with_context(|| format!("apply → {}", out_path.display()))?;
            println!(
                "\nwrote: {} ({} units)",
                out_path.display(),
                overrides.len()
            );
        }
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

    if write_blocked {
        return Err(anyhow!(
            "{} hard finding(s) — write-back blocked",
            summary.hard,
        ));
    }
    Ok(())
}

fn export_batch(args: ExportBatchArgs) -> Result<()> {
    match (args.path, args.project) {
        (Some(path), None) => export_batch_single(path, args.locale, args.out, args.glossary),
        (None, Some(project_root)) => export_batch_project(project_root, args.locale, args.out),
        _ => Err(anyhow!(
            "specify either a catalog path or --project, not both"
        )),
    }
}

fn export_batch_single(
    path: PathBuf,
    locale_id: String,
    out: PathBuf,
    glossary_path: Option<PathBuf>,
) -> Result<()> {
    let locale =
        Locale::by_id(&locale_id).ok_or_else(|| anyhow!("unknown locale: {}", locale_id))?;

    let glossary = load_optional_glossary(glossary_path.as_deref())?;

    export_batch_one(
        &path,
        locale,
        &out,
        glossary.as_ref(),
        glossary_path.as_deref(),
    )?;

    println!(
        "{}",
        out.canonicalize().unwrap_or_else(|_| out.clone()).display()
    );
    Ok(())
}

/// Write one per-catalog export subfolder. Shared by single-catalog and project modes.
fn export_batch_one(
    catalog_path: &Path,
    locale: &Locale,
    out_dir: &Path,
    glossary: Option<&Glossary>,
    glossary_path: Option<&Path>,
) -> Result<()> {
    let catalog =
        extract(catalog_path).with_context(|| format!("extract {}", catalog_path.display()))?;

    let writable: Vec<Unit> = catalog
        .units()
        .iter()
        .filter(|u| u.state.is_writable())
        .cloned()
        .collect();

    if writable.is_empty() {
        eprintln!(
            "note: no writable units in {}; export folder will be empty",
            catalog_path.display()
        );
    }

    let file_hash = file_hash(catalog_path)?;
    let batch = Batch::new(BatchKey::new(&file_hash, 0), writable);
    let prompt = build_agent_prompt(locale, glossary);

    agent_batch::write_export(
        out_dir,
        &batch,
        locale,
        glossary,
        glossary_path,
        catalog_path,
        &prompt,
    )
    .with_context(|| format!("write export to {}", out_dir.display()))?;

    // Create an empty targets.jsonl so agents can append directly.
    let targets_path = out_dir.join("targets.jsonl");
    if !targets_path.exists() {
        std::fs::File::create(&targets_path)
            .with_context(|| format!("create {}", targets_path.display()))?;
    }

    Ok(())
}

fn export_batch_project(project_root: PathBuf, locale_id: String, out: PathBuf) -> Result<()> {
    use i18n_harness_project::CatalogFormat;
    use std::time::{SystemTime, UNIX_EPOCH};

    let (project, warnings) = Project::open(&project_root)
        .with_context(|| format!("open project {}", project_root.display()))?;
    for w in &warnings {
        eprintln!("warning: {w}");
    }

    let matching: Vec<_> = project
        .catalogs()
        .iter()
        .filter(|c| c.locale == locale_id)
        .collect();

    if matching.is_empty() {
        let present: Vec<String> = {
            let mut seen = std::collections::BTreeSet::new();
            for c in project.catalogs() {
                seen.insert(c.locale.clone());
            }
            seen.into_iter().collect()
        };
        return Err(anyhow!(
            "no catalogs for locale `{locale_id}` in project {}; \
             locales present: {}",
            project_root.display(),
            if present.is_empty() {
                "(none)".to_owned()
            } else {
                present.join(", ")
            },
        ));
    }

    let locale =
        Locale::by_id(&locale_id).ok_or_else(|| anyhow!("unknown locale: {}", locale_id))?;

    let glossary = project.glossary();

    std::fs::create_dir_all(&out)
        .with_context(|| format!("create output dir {}", out.display()))?;

    let mut subfolders: Vec<String> = Vec::new();

    for cat_ref in &matching {
        if cat_ref.format != CatalogFormat::QtTs {
            eprintln!(
                "warning: skipping {:?} catalog `{}` — only Qt catalogs are supported in \
                 export-batch for now",
                cat_ref.format, cat_ref.manifest_path
            );
            continue;
        }

        let slug = manifest_path_slug(&cat_ref.manifest_path, &subfolders);
        let subfolder = out.join(&slug);
        subfolders.push(slug.clone());

        let catalog_abs = Path::new(&cat_ref.absolute_path);
        export_batch_one(
            catalog_abs,
            locale,
            &subfolder,
            glossary,
            project.paths().glossary(),
        )?;
    }

    if subfolders.is_empty() {
        return Err(anyhow!(
            "no Qt catalogs matched locale `{locale_id}` in project {}; \
             non-Qt catalogs are not yet supported by export-batch",
            project_root.display(),
        ));
    }

    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Root README.md
    let manifest_paths: Vec<&str> = matching
        .iter()
        .filter(|c| c.format == CatalogFormat::QtTs)
        .map(|c| c.manifest_path.as_str())
        .collect();
    let readme = render_project_readme(
        &project_root.display().to_string(),
        &locale_id,
        &subfolders,
        &manifest_paths,
    );
    write_file_atomic(&out, "README.md", readme.as_bytes())?;

    // Root meta.json
    let meta = serde_json::json!({
        "format_version": "1",
        "mode": "project",
        "project_root": project_root.display().to_string(),
        "locale_id": locale_id,
        "subfolders": subfolders,
        "started_at_unix_secs": started_at,
    });
    let meta_bytes = {
        let mut v =
            serde_json::to_vec_pretty(&meta).map_err(|e| anyhow!("serialize meta.json: {e}"))?;
        v.push(b'\n');
        v
    };
    write_file_atomic(&out, "meta.json", &meta_bytes)?;

    println!(
        "{}",
        out.canonicalize().unwrap_or_else(|_| out.clone()).display()
    );
    Ok(())
}

/// Derive a filesystem-safe slug from a manifest path, avoiding collisions.
fn manifest_path_slug(manifest_path: &str, existing: &[String]) -> String {
    // Replace all forbidden characters with `_`; keep `.`.
    let forbidden = |c: char| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|');
    let raw: String = manifest_path
        .chars()
        .map(|c| if forbidden(c) { '_' } else { c })
        .collect();

    // Strip a leading dot so we never create hidden folders.
    let raw = raw.trim_start_matches('.').to_owned();

    // Cap at 200 chars (reserve room for collision suffixes).
    const MAX_LEN: usize = 200;
    let base = if raw.len() <= MAX_LEN {
        raw
    } else {
        use sha2::{Digest, Sha256};
        use std::fmt::Write as _;
        let mut h = Sha256::new();
        h.update(manifest_path.as_bytes());
        let hash = format!("{:x}", h.finalize());
        let mut s = raw[..MAX_LEN - 9].to_owned();
        let _ = write!(s, "_{}", &hash[..8]);
        s
    };

    // Collision avoidance.
    if !existing.contains(&base) {
        return base;
    }
    for n in 1u32.. {
        let candidate = format!("{base}-{n}");
        if !existing.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!("collision loop exhausted u32 space")
}

fn render_project_readme(
    project_root: &str,
    locale_id: &str,
    subfolders: &[String],
    manifest_paths: &[&str],
) -> String {
    let rows: String = subfolders
        .iter()
        .zip(manifest_paths.iter())
        .map(|(sf, mp)| format!("| `{sf}/` | `{mp}` |\n"))
        .collect();
    format!(
        r#"# Project translation batch — {locale_id}

Project root: `{project_root}`
Locale: **{locale_id}**

## Catalog subfolders

| Subfolder | Manifest path |
|---|---|
{rows}
Each subfolder contains the same layout as a single-catalog batch
(`units.jsonl`, `prompt.md`, `targets.jsonl`, `meta.json`).

See `skills/translate-i18n-batch/` for the rule-set and schema.
"#
    )
}

fn write_file_atomic(dir: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    let final_path = dir.join(name);
    let tmp_path = dir.join(format!("{name}.tmp"));
    {
        use std::io::Write as _;
        let mut f = std::fs::File::create(&tmp_path)
            .with_context(|| format!("create {}", tmp_path.display()))?;
        f.write_all(bytes)
            .with_context(|| format!("write {}", tmp_path.display()))?;
        f.sync_all()
            .with_context(|| format!("sync {}", tmp_path.display()))?;
    }
    std::fs::rename(&tmp_path, &final_path)
        .with_context(|| format!("rename {} → {}", tmp_path.display(), final_path.display()))?;
    Ok(())
}

/// Build the system-prompt string inlined into `prompt.md`.
///
/// Glossary content is inlined here so the agent can read everything it
/// needs from a single file without loading the project glossary separately.
fn build_agent_prompt(locale: &Locale, glossary: Option<&Glossary>) -> String {
    use i18n_harness_glossary::Register as GlossRegister;

    let register =
        glossary
            .and_then(|g| g.register_for(locale.id))
            .unwrap_or(match locale.register {
                i18n_harness_locales::Register::Formal => GlossRegister::Formal,
                i18n_harness_locales::Register::Informal => GlossRegister::Informal,
                i18n_harness_locales::Register::Neutral => GlossRegister::Neutral,
            });

    let register_str = match register {
        GlossRegister::Formal => "formal",
        GlossRegister::Informal => "informal",
        GlossRegister::Neutral => "neutral",
    };

    let terms_block = match glossary {
        Some(g) => {
            let terms: Vec<_> = g.terms_for(locale.id).collect();
            if terms.is_empty() {
                "(none)".to_owned()
            } else {
                terms
                    .into_iter()
                    .map(|(src, tgt)| format!("  {src} → {tgt}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        None => "(none)".to_owned(),
    };

    let dnt_block = match glossary {
        Some(g) => {
            let dnt: Vec<_> = g.do_not_translate().collect();
            if dnt.is_empty() {
                "(none)".to_owned()
            } else {
                dnt.iter()
                    .map(|s| format!("  - {s}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        None => "(none)".to_owned(),
    };

    format!(
        r#"# System prompt — agent translation batch

Target locale: {locale_id} ({script:?})
Register: {register_str}

Glossary terms (source → target):
{terms_block}

Do not translate:
{dnt_block}

See README.md in this folder for the schema of targets.jsonl and the full rules.
If you are running the `translate-i18n-batch` Claude Code skill, it has the full rule-set.
"#,
        locale_id = locale.id,
        script = locale.script,
    )
}

fn import_batch(args: ImportBatchArgs) -> Result<()> {
    match (args.apply, args.project) {
        (Some(apply_path), None) => {
            import_batch_single(args.dir, apply_path, args.metrics, args.out)
        }
        (None, Some(project_root)) => {
            import_batch_project(args.dir, project_root, args.metrics, args.out_dir)
        }
        (Some(_), Some(_)) => Err(anyhow!("specify either --apply or --project, not both")),
        (None, None) => Err(anyhow!(
            "specify either --apply <catalog> or --project <dir>"
        )),
    }
}

fn import_batch_single(
    dir: PathBuf,
    apply_path: PathBuf,
    metrics: Option<PathBuf>,
    out: Option<PathBuf>,
) -> Result<()> {
    let (locale_id, _mode) = read_meta_locale_and_mode(&dir)?;
    let locale = Locale::by_id(&locale_id)
        .ok_or_else(|| anyhow!("unknown locale in meta.json: {locale_id}"))?;

    let responses = agent_batch::read_targets(&dir).map_err(|e| anyhow!("{e}"))?;

    let catalog =
        extract(&apply_path).with_context(|| format!("extract {}", apply_path.display()))?;
    let original_units = catalog.units().to_vec();

    let mut writable: Vec<Unit> = Vec::new();
    let mut preserved: Vec<Unit> = Vec::new();
    for u in &original_units {
        if u.state.is_writable() {
            writable.push(u.clone());
        } else {
            preserved.push(u.clone());
        }
    }

    if writable.len() != responses.len() {
        return Err(anyhow!(
            "catalog writable unit count ({writable}) differs from targets.jsonl line count ({got}); \
             the catalog may have changed since export-batch ran",
            writable = writable.len(),
            got = responses.len(),
        ));
    }

    let file_hash = file_hash(&apply_path)?;
    let batch = Batch::new(BatchKey::new(&file_hash, 0), writable);

    verify_batch_matches_export(&dir, &batch)?;

    let response_map: HashMap<String, ManualResponse> = batch
        .units
        .iter()
        .map(|u| u.id.as_str().to_owned())
        .zip(responses)
        .collect();

    let backend = ManualBackend::named("agent", false, |ctx| {
        response_map
            .get(ctx.unit.id.as_str())
            .cloned()
            .unwrap_or(ManualResponse::Skip)
    });
    let backend_name = backend.name().to_owned();

    let writer = metrics
        .as_ref()
        .map(|p| MetricsWriter::new(&backend_name, &locale_id, FileSink::new(p)));
    let outcomes = backend
        .translate_batch(&batch, locale, None)
        .with_context(|| "agent backend translate")?;

    if outcomes.len() != batch.units.len() {
        return Err(anyhow!(
            "backend returned {got} outcomes for batch of {expected}",
            got = outcomes.len(),
            expected = batch.units.len(),
        ));
    }

    let mut summary = TranslateSummary::default();
    let merged: Vec<Unit> = batch
        .units
        .iter()
        .zip(outcomes.iter())
        .map(|(unit, outcome)| merge_outcome(unit, outcome, &mut summary))
        .collect();

    let reports = validate_batch(&merged, locale, None);

    let mut translated_units: Vec<Unit> = Vec::with_capacity(merged.len());
    for (unit, report) in merged.into_iter().zip(reports) {
        if let Some(w) = writer.as_ref()
            && !report.findings.is_empty()
            && let Err(e) = w.record_report(&report)
        {
            eprintln!("warning: failed to write metrics line: {e}");
        }

        let mut promoted = unit;
        if report.is_clean() && promoted.target.is_complete() {
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

    let mut overrides = translated_units;
    overrides.extend(preserved);

    let _bytes =
        render(&catalog, &overrides).with_context(|| format!("render {}", apply_path.display()))?;

    let write_blocked = summary.hard > 0;

    if let Some(out_path) = out.as_ref() {
        if write_blocked {
            eprintln!(
                "\nblocked: {} hard finding(s); not writing {} (use `harness gate` to inspect)",
                summary.hard,
                out_path.display(),
            );
        } else {
            apply(&catalog, &overrides, out_path)
                .with_context(|| format!("apply → {}", out_path.display()))?;
            println!(
                "\nwrote: {} ({} units)",
                out_path.display(),
                overrides.len()
            );
        }
    } else {
        println!("\ndry-run (no --out); skipped write-back");
    }

    println!(
        "translate report: {} → locale={} backend={}  writable={} translated={} skipped={} failed={} finished={} flagged={} hard={} soft={}",
        apply_path.display(),
        locale_id,
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

    if write_blocked {
        return Err(anyhow!(
            "{} hard finding(s) — write-back blocked",
            summary.hard,
        ));
    }
    Ok(())
}

fn import_batch_project(
    batch_root: PathBuf,
    project_root: PathBuf,
    metrics: Option<PathBuf>,
    out_dir: Option<PathBuf>,
) -> Result<()> {
    // Read root meta.json; confirm mode == "project".
    let root_meta_path = batch_root.join("meta.json");
    let root_meta_bytes = std::fs::read(&root_meta_path)
        .with_context(|| format!("read {}", root_meta_path.display()))?;
    let root_meta: serde_json::Value = serde_json::from_slice(&root_meta_bytes)
        .with_context(|| format!("parse {}", root_meta_path.display()))?;

    let mode = root_meta.get("mode").and_then(|v| v.as_str()).unwrap_or("");
    if mode != "project" {
        return Err(anyhow!(
            "batch at {} was produced in single-catalog mode (mode={:?}); \
             use `--apply <catalog>` instead of `--project`",
            batch_root.display(),
            mode,
        ));
    }

    let locale_id = root_meta
        .get("locale_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("root meta.json missing locale_id"))?
        .to_owned();
    let locale = Locale::by_id(&locale_id)
        .ok_or_else(|| anyhow!("unknown locale in meta.json: {locale_id}"))?;

    let subfolders: Vec<String> = root_meta
        .get("subfolders")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("root meta.json missing subfolders array"))?
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();

    let (project, warnings) = Project::open(&project_root)
        .with_context(|| format!("open project {}", project_root.display()))?;
    for w in &warnings {
        eprintln!("warning: {w}");
    }

    let backend_name = "agent";
    let writer = metrics
        .as_ref()
        .map(|p| MetricsWriter::new(backend_name, &locale_id, FileSink::new(p)));

    let mut overall_hard = 0usize;
    let mut overall_summary = TranslateSummary::default();

    for subfolder_name in &subfolders {
        let subfolder = batch_root.join(subfolder_name);

        // Read the subfolder's meta.json to find the original catalog path.
        let sub_meta_path = subfolder.join("meta.json");
        let sub_meta_bytes = std::fs::read(&sub_meta_path)
            .with_context(|| format!("read {}", sub_meta_path.display()))?;
        let sub_meta: serde_json::Value = serde_json::from_slice(&sub_meta_bytes)
            .with_context(|| format!("parse {}", sub_meta_path.display()))?;

        let source_catalog_path_str = sub_meta
            .get("source_catalog_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                anyhow!(
                    "subfolder meta.json {} missing source_catalog_path",
                    sub_meta_path.display()
                )
            })?;
        let catalog_abs = Path::new(source_catalog_path_str);

        // Match against the project by manifest_path or absolute_path.
        let cat_ref = project.catalog(catalog_abs).ok_or_else(|| {
            anyhow!(
                "catalog `{}` (from subfolder `{}`) is not registered in project {}",
                source_catalog_path_str,
                subfolder_name,
                project_root.display(),
            )
        })?;

        let catalog_abs_path = Path::new(&cat_ref.absolute_path);

        let responses = agent_batch::read_targets(&subfolder).map_err(|e| anyhow!("{e}"))?;

        let catalog = extract(catalog_abs_path)
            .with_context(|| format!("extract {}", catalog_abs_path.display()))?;
        let original_units = catalog.units().to_vec();

        let mut writable: Vec<Unit> = Vec::new();
        let mut preserved: Vec<Unit> = Vec::new();
        for u in &original_units {
            if u.state.is_writable() {
                writable.push(u.clone());
            } else {
                preserved.push(u.clone());
            }
        }

        if writable.len() != responses.len() {
            return Err(anyhow!(
                "catalog `{}` writable unit count ({}) differs from targets.jsonl count ({}); \
                 the catalog may have changed since export-batch ran",
                cat_ref.manifest_path,
                writable.len(),
                responses.len(),
            ));
        }

        let file_hash = file_hash(catalog_abs_path)?;
        let batch = Batch::new(BatchKey::new(&file_hash, 0), writable);

        verify_batch_matches_export(&subfolder, &batch).with_context(|| {
            format!(
                "subfolder `{subfolder_name}` (catalog `{}`)",
                cat_ref.manifest_path
            )
        })?;

        let response_map: HashMap<String, ManualResponse> = batch
            .units
            .iter()
            .map(|u| u.id.as_str().to_owned())
            .zip(responses)
            .collect();

        let backend = ManualBackend::named(backend_name, false, |ctx| {
            response_map
                .get(ctx.unit.id.as_str())
                .cloned()
                .unwrap_or(ManualResponse::Skip)
        });

        let outcomes = backend
            .translate_batch(&batch, locale, None)
            .with_context(|| format!("translate batch for `{}`", cat_ref.manifest_path))?;

        let mut summary = TranslateSummary::default();
        let merged: Vec<Unit> = batch
            .units
            .iter()
            .zip(outcomes.iter())
            .map(|(unit, outcome)| merge_outcome(unit, outcome, &mut summary))
            .collect();

        let reports = validate_batch(&merged, locale, None);

        let mut translated_units: Vec<Unit> = Vec::with_capacity(merged.len());
        for (unit, report) in merged.into_iter().zip(reports) {
            if let Some(w) = writer.as_ref()
                && !report.findings.is_empty()
                && let Err(e) = w.record_report(&report)
            {
                eprintln!("warning: failed to write metrics line: {e}");
            }

            let mut promoted = unit;
            if report.is_clean() && promoted.target.is_complete() {
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

        let mut overrides = translated_units;
        overrides.extend(preserved);

        let _bytes = render(&catalog, &overrides)
            .with_context(|| format!("render {}", catalog_abs_path.display()))?;

        let catalog_write_blocked = summary.hard > 0;

        if let Some(out_base) = out_dir.as_ref() {
            let manifest_rel = Path::new(&cat_ref.manifest_path);
            let dest = out_base.join(manifest_rel);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("create dirs {}", parent.display()))?;
            }

            if catalog_write_blocked {
                eprintln!(
                    "\nblocked: {} hard finding(s) in `{}`; not writing {} \
                     (use `harness gate` to inspect)",
                    summary.hard,
                    cat_ref.manifest_path,
                    dest.display(),
                );
            } else {
                apply(&catalog, &overrides, &dest)
                    .with_context(|| format!("apply → {}", dest.display()))?;
                println!("\nwrote: {} ({} units)", dest.display(), overrides.len());
            }
        }

        println!(
            "translate report: {} → locale={} backend={}  writable={} translated={} skipped={} \
             failed={} finished={} flagged={} hard={} soft={}",
            cat_ref.manifest_path,
            locale_id,
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

        overall_hard += summary.hard;
        overall_summary.writable_total += summary.writable_total;
        overall_summary.translated += summary.translated;
        overall_summary.skipped += summary.skipped;
        overall_summary.failed += summary.failed;
        overall_summary.finished += summary.finished;
        overall_summary.flagged += summary.flagged;
        overall_summary.hard += summary.hard;
        overall_summary.soft += summary.soft;
    }

    if out_dir.is_none() {
        println!("\ndry-run (no --out-dir); skipped write-back");
    }

    println!(
        "\noverall report: catalogs={} locale={} backend={}  writable={} translated={} \
         skipped={} failed={} finished={} flagged={} hard={} soft={}",
        subfolders.len(),
        locale_id,
        backend_name,
        overall_summary.writable_total,
        overall_summary.translated,
        overall_summary.skipped,
        overall_summary.failed,
        overall_summary.finished,
        overall_summary.flagged,
        overall_summary.hard,
        overall_summary.soft,
    );

    if overall_hard > 0 {
        return Err(anyhow!(
            "{} hard finding(s) across project — some catalogs were not written",
            overall_hard,
        ));
    }
    Ok(())
}

fn read_meta_locale_and_mode(dir: &Path) -> Result<(String, String)> {
    let meta_path = dir.join("meta.json");
    let meta_bytes =
        std::fs::read(&meta_path).with_context(|| format!("read {}", meta_path.display()))?;
    let meta: serde_json::Value = serde_json::from_slice(&meta_bytes)
        .with_context(|| format!("parse {}", meta_path.display()))?;
    let locale_id = meta
        .get("locale_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("meta.json missing locale_id"))?
        .to_owned();
    let mode = meta
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("single")
        .to_owned();
    Ok((locale_id, mode))
}

fn load_optional_glossary(path: Option<&Path>) -> Result<Option<Glossary>> {
    match path {
        Some(p) => {
            let (g, warnings) =
                Glossary::load(p).with_context(|| format!("load glossary {}", p.display()))?;
            for w in &warnings {
                eprintln!("glossary warning: {w}");
            }
            Ok(Some(g))
        }
        None => Ok(None),
    }
}

/// Verify the current catalog's writable units (after `Batch::new` sorting)
/// match the ids recorded in the export folder's `units.jsonl`, position-wise.
///
/// `read_targets` already validates that `targets.jsonl` lines align with
/// `units.jsonl`, but that only proves the export folder is internally
/// consistent. If the source catalog has been edited (units added, removed,
/// renamed, or re-keyed) between `export-batch` and `import-batch`, the
/// writable unit count can still match the response count by coincidence
/// while the per-position ids drift. Position-wise application would then
/// silently translate the wrong units. Refuse to proceed instead.
fn verify_batch_matches_export(export_dir: &Path, batch: &Batch) -> Result<()> {
    let expected_ids =
        agent_batch::read_unit_ids(export_dir).map_err(|e| anyhow!("read units.jsonl: {e}"))?;
    if expected_ids.len() != batch.units.len() {
        // Already covered upstream by the writable/response count check,
        // but a second guard here keeps this helper self-contained.
        return Err(anyhow!(
            "export folder lists {} units; current catalog has {} writable units",
            expected_ids.len(),
            batch.units.len(),
        ));
    }
    for (idx, (current, expected)) in batch.units.iter().zip(expected_ids.iter()).enumerate() {
        if current.id.as_str() != expected {
            return Err(anyhow!(
                "unit id mismatch at position {idx}: export expects `{expected}`, \
                 current catalog has `{current_id}`. The catalog has changed since \
                 export-batch ran; re-run export-batch and try again.",
                current_id = current.id.as_str(),
            ));
        }
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
        TranslationOutcome::Translated {
            text,
            flags,
            confidence,
            flag_notes,
        } => {
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
            out.confidence = *confidence;
            out.flag_notes = flag_notes.clone();
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

fn init(args: InitArgs) -> Result<()> {
    use i18n_harness_project::{ClassificationConfidence, Project};

    let dir = args.dir;
    if !dir.is_dir() {
        return Err(anyhow!("{} is not a directory", dir.display()));
    }
    let manifest_path = dir.join("i18n-harness.toml");
    if manifest_path.exists() && !args.force {
        return Err(anyhow!(
            "{} already exists; pass --force to overwrite",
            manifest_path.display()
        ));
    }

    let draft = Project::discover(&dir).with_context(|| format!("discover {}", dir.display()))?;

    println!("Discovered project at {}", dir.display());
    println!("  name:     {}", draft.name);
    println!("  locales:  {}", format_locales(&draft.locales));
    println!("  catalogs: {}", draft.catalogs.len());
    for c in &draft.catalogs {
        let rel = c.path.strip_prefix(&dir).unwrap_or(&c.path);
        let conf = match c.confidence {
            ClassificationConfidence::High => "high",
            ClassificationConfidence::Medium => "medium",
            ClassificationConfidence::Low => "low",
        };
        let loc = c.locale.as_deref().unwrap_or("?");
        println!(
            "    {:>6}  {:<10}  {:<8}  {}",
            conf,
            format!("{:?}", c.format),
            loc,
            rel.display()
        );
    }
    if draft.glossary.is_some() {
        println!("  glossary: glossary.toml");
    }

    let (project, warnings) = Project::create_from_draft(&dir, draft)
        .with_context(|| format!("create_from_draft {}", dir.display()))?;
    for w in &warnings {
        eprintln!("warning: {w}");
    }
    println!(
        "Wrote {} ({} catalogs, {} locales)",
        project.paths().manifest().display(),
        project.catalogs().len(),
        project.locale_ids().count(),
    );
    Ok(())
}

fn open(args: OpenArgs) -> Result<()> {
    use i18n_harness_project::Project;

    let dir = args.dir;
    let (project, warnings) =
        Project::open(&dir).with_context(|| format!("open project {}", dir.display()))?;
    for w in &warnings {
        eprintln!("warning: {w}");
    }
    let s = project.summary();
    println!("Project:    {}", s.name);
    println!("Root:       {}", s.root);
    println!("Schema:     {}", s.schema);
    println!("Locales:    {}", s.locales.join(", "));
    println!("Catalogs:   {}", s.catalogs.len());
    for c in &s.catalogs {
        println!(
            "  {:<10}  {:<8}  {}",
            format!("{:?}", c.format),
            c.locale,
            c.manifest_path
        );
    }
    if let Some(g) = &s.glossary_path {
        println!("Glossary:   {g}");
    }
    if let Some(b) = &s.backend {
        let model = b.model.as_deref().unwrap_or("-");
        let host = b.host.as_deref().unwrap_or("-");
        println!("Backend:    {:?} model={model} host={host}", b.kind);
    }
    println!("State dir:  {}", s.state_dir);
    Ok(())
}

fn format_locales(
    locales: &std::collections::BTreeMap<String, i18n_harness_project::LocaleConfig>,
) -> String {
    if locales.is_empty() {
        "—".to_string()
    } else {
        locales.keys().cloned().collect::<Vec<_>>().join(", ")
    }
}

fn reuse(args: ReuseArgs) -> Result<()> {
    match (args.path, args.project) {
        (Some(path), None) => {
            reuse_single(path, args.locale, args.references, args.out, args.glossary)
        }
        (None, Some(project_root)) => reuse_project(project_root, args.locale),
        _ => Err(anyhow!(
            "specify either a catalog path or --project, not both"
        )),
    }
}

fn reuse_single(
    path: PathBuf,
    locale_id: String,
    reference_paths: Vec<PathBuf>,
    out: Option<PathBuf>,
    glossary_path: Option<PathBuf>,
) -> Result<()> {
    if reference_paths.is_empty() {
        return Err(anyhow!(
            "ad-hoc mode requires at least one --reference <path>"
        ));
    }
    let locale = Locale::by_id(&locale_id).ok_or_else(|| anyhow!("unknown locale: {locale_id}"))?;
    let glossary = load_optional_glossary(glossary_path.as_deref())?;

    let outcome = reuse_from_references(&path, &reference_paths, locale, glossary.as_ref())
        .map_err(|e| match e {
            ReuseError::Extract { path: p, source } => {
                anyhow!("extract {}: {source}", p.display())
            }
            other => anyhow!("{other}"),
        })?;

    let out_path = out.as_deref().unwrap_or(&path);
    apply(&outcome.base, &outcome.units, out_path)
        .with_context(|| format!("apply → {}", out_path.display()))?;

    print_reuse_report(&path, &outcome.report);
    Ok(())
}

fn reuse_project(project_root: PathBuf, locale_id: String) -> Result<()> {
    use i18n_harness_project::CatalogFormat;

    let (project, warnings) = Project::open(&project_root)
        .with_context(|| format!("open project {}", project_root.display()))?;
    for w in &warnings {
        eprintln!("warning: {w}");
    }

    let locale = Locale::by_id(&locale_id).ok_or_else(|| anyhow!("unknown locale: {locale_id}"))?;

    let matching_bases: Vec<_> = project
        .catalogs()
        .iter()
        .filter(|c| c.locale == locale_id)
        .collect();

    if matching_bases.is_empty() {
        let present: Vec<String> = {
            let mut seen = std::collections::BTreeSet::new();
            for c in project.catalogs() {
                seen.insert(c.locale.clone());
            }
            seen.into_iter().collect()
        };
        return Err(anyhow!(
            "no catalogs for locale `{locale_id}` in project {}; \
             locales present: {}",
            project_root.display(),
            if present.is_empty() {
                "(none)".to_owned()
            } else {
                present.join(", ")
            },
        ));
    }

    let reference_paths: Vec<PathBuf> = project
        .references()
        .iter()
        .filter(|r| r.locale == locale_id && r.format == CatalogFormat::QtTs)
        .map(|r| PathBuf::from(&r.absolute_path))
        .collect();

    if reference_paths.is_empty() {
        eprintln!(
            "note: no Qt references for locale `{locale_id}` in project {}; \
             nothing to reuse",
            project_root.display(),
        );
    }

    let glossary = project.glossary();

    for cat_ref in &matching_bases {
        if cat_ref.format != CatalogFormat::QtTs {
            eprintln!(
                "warning: skipping {:?} catalog `{}` — only Qt catalogs are supported in \
                 reuse for now",
                cat_ref.format, cat_ref.manifest_path
            );
            continue;
        }

        let base_path = Path::new(&cat_ref.absolute_path);

        let outcome = reuse_from_references(base_path, &reference_paths, locale, glossary)
            .map_err(|e| match e {
                ReuseError::Extract { path: p, source } => {
                    anyhow!("extract {}: {source}", p.display())
                }
                other => anyhow!("{other}"),
            })?;

        apply(&outcome.base, &outcome.units, base_path)
            .with_context(|| format!("apply → {}", base_path.display()))?;

        print_reuse_report(base_path, &outcome.report);
    }

    Ok(())
}

fn print_reuse_report(base: &Path, report: &i18n_harness_reuse::ReuseReport) {
    println!(
        "\nreuse report: {}  copied_finished={}  copied_needs_review={}  conflicts={}  remaining={}",
        base.display(),
        report.copied_finished_count(),
        report.copied_needs_review_count(),
        report.conflict_count(),
        report.remaining_count(),
    );

    if !report.copied.is_empty() {
        println!("  copied provenance:");
        for cu in &report.copied {
            let disposition = match cu.disposition {
                i18n_harness_reuse::CopiedDisposition::Finished => "finished",
                i18n_harness_reuse::CopiedDisposition::NeedsReview => "needs-review",
            };
            println!(
                "    {} ← {} [{disposition}]",
                cu.id.as_str(),
                cu.winning_reference.display(),
            );
        }
    }

    if !report.conflicts.is_empty() {
        println!("  conflicts (left untranslated — pick a candidate manually):");
        for conflict in &report.conflicts {
            println!("    {}:", conflict.id.as_str());
            for (i, cand) in conflict.candidates.iter().enumerate() {
                let text_repr = match &cand.text {
                    i18n_harness_reuse::ConflictText::Singular(s) => format!("{s:?}"),
                    i18n_harness_reuse::ConflictText::Plural(forms) => {
                        format!("{forms:?}")
                    }
                };
                println!(
                    "      candidate {}: {} → {text_repr}",
                    i + 1,
                    cand.reference.display(),
                );
                for also in &cand.also_from {
                    println!("        (also from {})", also.display());
                }
            }
        }
    }
}

fn split_remainder(args: SplitRemainderArgs) -> Result<()> {
    let catalog =
        extract(&args.base).with_context(|| format!("extract {}", args.base.display()))?;
    let keep = writable_untranslated_ids(&catalog);
    let count = keep.len();

    if let Some(parent) = args.out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create dirs {}", parent.display()))?;
        }
    }

    write_subset(&catalog, &keep, &args.out)
        .with_context(|| format!("write_subset → {}", args.out.display()))?;

    println!(
        "split-remainder: {} → {} ({count} writable-untranslated unit(s))",
        args.base.display(),
        args.out.display(),
    );
    Ok(())
}

fn merge(args: MergeArgs) -> Result<()> {
    let outcome = merge_back(&args.base, &args.remainder).map_err(|e| match e {
        ReuseError::MergeOverlap { base, ids } => {
            anyhow!(
                "{n} unit id(s) are finished in both base {base} and remainder; \
                 re-derive the halves from the current base. ids: {ids}",
                n = ids.len(),
                base = base.display(),
                ids = ids
                    .iter()
                    .map(|id| format!("`{id}`"))
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        }
        ReuseError::MergeStrayIds { base, ids } => {
            anyhow!(
                "{n} remainder unit id(s) not present in base {base}; \
                 re-run split-remainder from the current base and re-translate. ids: {ids}",
                n = ids.len(),
                base = base.display(),
                ids = ids
                    .iter()
                    .map(|id| format!("`{id}`"))
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        }
        ReuseError::Extract { path, source } => {
            anyhow!("extract {}: {source}", path.display())
        }
        ReuseError::UnknownLocale(id) => anyhow!("unknown locale: {id}"),
    })?;

    if let Some(parent) = args.out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create dirs {}", parent.display()))?;
        }
    }

    apply(&outcome.base, &outcome.units, &args.out)
        .with_context(|| format!("apply → {}", args.out.display()))?;

    println!(
        "merge: {} + {} → {} (merged={} merged_complete={})",
        args.base.display(),
        args.remainder.display(),
        args.out.display(),
        outcome.report.merged,
        outcome.report.merged_complete,
    );
    Ok(())
}

fn file_hash(path: &std::path::Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).with_context(|| format!("read {} for hash", path.display()))?;
    // SHA-256 hex (lower-case). The `core::Batch` contract treats the
    // hash as an opaque string; we commit to a specific algorithm here
    // so on-disk resume-key persistence stays consistent. Using a
    // cryptographic hash from the start avoids a forced migration
    // once persisted state references it.
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}
