//! `harness` — the headless CLI entry point for `i18n-harness`.
//!
//! See `docs/initial_design.md`. Phase 0 stub — `extract`, `apply`,
//! `round-trip`, `translate`, `export-batch`, `import-batch` subcommands
//! land milestone by milestone (see plan in `/Users/vitalyvorobyev/.claude/plans/`).

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "harness",
    version,
    about = "i18n-harness CLI (phase 0 stub)",
    long_about = "Local-first translation harness. See docs/initial_design.md for the design \
                  intent. Subcommands land milestone by milestone."
)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
    println!("i18n-harness CLI: phase 0 stub. See docs/initial_design.md.");
}
