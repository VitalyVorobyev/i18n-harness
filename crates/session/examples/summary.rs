//! Print what a session sees in a folder: `cargo run -p i18n-harness-session
//! --example summary -- <folder> [pivot-locale]`. Opening a folder without
//! settings creates its `i18n-harness.toml`.

use std::path::{Path, PathBuf};

use i18n_harness_session::Session;

fn main() {
    let mut args = std::env::args().skip(1);
    let folder = args.next().expect("usage: summary <folder> [pivot-locale]");
    let pivot = args.next();
    let mut session = match Session::open_folder(Path::new(&folder)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot open {folder}: {e}");
            std::process::exit(1);
        }
    };
    if let Some(pivot) = pivot.as_deref() {
        let paths: Vec<PathBuf> = session
            .catalogs()
            .into_iter()
            .map(|c| c.path.into())
            .collect();
        for path in paths {
            session.set_pivot(&path, Some(pivot)).expect("set pivot");
        }
    }
    for w in session.warnings() {
        println!("warning: {w}");
    }
    for c in session.catalogs() {
        println!(
            "{:<22} {:<12} {:<7} pivot={:<7} total={:<5} to-do={:<5} check={:<4} done={:<5} errors={}",
            c.name,
            c.format_name,
            c.locale,
            c.pivot.as_deref().unwrap_or("-"),
            c.counts.total,
            c.counts.to_do,
            c.counts.check,
            c.counts.done,
            c.counts.errors
        );
    }
}
