//! Directory walker for the discovery heuristic.
//!
//! Recurses from a root directory, honoring a skip-list of directory names
//! (`.git`, `node_modules`, `target`, etc.) and a maximum depth of 8. See
//! design §3.1.

use std::path::{Path, PathBuf};

use crate::fs::ProjectFs;

/// Maximum recursion depth. Depth 0 = root itself; depth 8 = eight levels in.
pub(crate) const MAX_DEPTH: usize = 8;

/// Directory names that are always skipped during discovery.
///
/// Hidden directories (names starting with `.`) are skipped except `.config`,
/// which is a common location for project configuration. `.i18n-harness` is
/// skipped explicitly to avoid accidentally reading our own state files.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    "__pycache__",
    ".i18n-harness",
];

/// File extensions that are candidates for catalog format detection.
const CANDIDATE_EXTENSIONS: &[&str] = &["ts", "po", "pot", "json"];

/// Whether `name` should be skipped during directory recursion.
pub(crate) fn should_skip_dir(name: &str) -> bool {
    if SKIP_DIRS.contains(&name) {
        return true;
    }
    // Skip hidden directories (starting with `.`) except `.config`.
    if name.starts_with('.') && name != ".config" {
        return true;
    }
    false
}

/// Walk `root` recursively using `fs`, collecting candidate file paths.
///
/// - Respects [`should_skip_dir`] at every directory boundary.
/// - Stops recursing when depth exceeds [`MAX_DEPTH`].
/// - Only files whose extension is in [`CANDIDATE_EXTENSIONS`] are included.
///
/// Returns an unsorted list of absolute paths to candidate files.
pub(crate) fn walk_candidates(root: &Path, fs: &dyn ProjectFs) -> Vec<PathBuf> {
    let mut results = Vec::new();
    walk_recursive(root, 0, fs, &mut results);
    results
}

fn walk_recursive(dir: &Path, depth: usize, fs: &dyn ProjectFs, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }

    let entries = match fs.list_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries {
        let name = entry
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        if fs.is_dir(&entry) {
            if !should_skip_dir(name) {
                walk_recursive(&entry, depth + 1, fs, out);
            }
        } else if fs.is_file(&entry) {
            let ext = entry
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if CANDIDATE_EXTENSIONS.contains(&ext.as_str()) {
                out.push(entry);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::fs::InMemoryFs;

    fn make_fs() -> Arc<InMemoryFs> {
        Arc::new(InMemoryFs::new())
    }

    #[test]
    fn skips_git_dir() {
        let fs = make_fs();
        let root = Path::new("/proj");
        fs.create_dir_all(&root.join(".git/objects")).unwrap();
        fs.write_atomic(&root.join(".git/objects/fake.ts"), b"<TS></TS>")
            .unwrap();
        fs.write_atomic(&root.join("app.ts"), b"<TS></TS>").unwrap();

        let candidates = walk_candidates(root, &*fs);
        assert!(
            candidates.iter().all(|p| !p.starts_with(root.join(".git"))),
            "no files inside .git should appear: {candidates:?}"
        );
        assert!(candidates.contains(&root.join("app.ts")));
    }

    #[test]
    fn skips_node_modules() {
        let fs = make_fs();
        let root = Path::new("/proj");
        fs.write_atomic(&root.join("node_modules/de.json"), b"{\"k\":\"v\"}")
            .unwrap();
        fs.write_atomic(&root.join("translations/de.json"), b"{\"k\":\"v\"}")
            .unwrap();

        let candidates = walk_candidates(root, &*fs);
        assert!(
            candidates
                .iter()
                .all(|p| !p.starts_with(root.join("node_modules"))),
        );
        assert!(candidates.contains(&root.join("translations/de.json")));
    }

    #[test]
    fn max_depth_respected() {
        let fs = make_fs();
        let root = Path::new("/proj");

        // Build a path at depth 8 (allowed) and depth 9 (skipped).
        let at_8 = root.join("a/b/c/d/e/f/g/h");
        let at_9 = root.join("a/b/c/d/e/f/g/h/i");
        fs.write_atomic(&at_8.join("ok.ts"), b"<TS></TS>").unwrap();
        fs.write_atomic(&at_9.join("skip.ts"), b"<TS></TS>").unwrap();

        let candidates = walk_candidates(root, &*fs);
        assert!(candidates.contains(&at_8.join("ok.ts")), "depth-8 file must be found");
        assert!(
            !candidates.contains(&at_9.join("skip.ts")),
            "depth-9 file must be skipped"
        );
    }

    #[test]
    fn hidden_dirs_skipped_except_config() {
        let fs = make_fs();
        let root = Path::new("/proj");
        fs.write_atomic(&root.join(".hidden/secret.ts"), b"<TS></TS>")
            .unwrap();
        fs.write_atomic(&root.join(".config/app_de.ts"), b"<TS></TS>")
            .unwrap();

        let candidates = walk_candidates(root, &*fs);
        assert!(
            !candidates.contains(&root.join(".hidden/secret.ts")),
            ".hidden should be skipped"
        );
        assert!(
            candidates.contains(&root.join(".config/app_de.ts")),
            ".config should NOT be skipped"
        );
    }

    #[test]
    fn only_candidate_extensions_returned() {
        let fs = make_fs();
        let root = Path::new("/proj");
        fs.write_atomic(&root.join("README.md"), b"# readme").unwrap();
        fs.write_atomic(&root.join("Makefile"), b"all:").unwrap();
        fs.write_atomic(&root.join("de.po"), b"msgid\nmsgstr").unwrap();

        let candidates = walk_candidates(root, &*fs);
        assert!(candidates.contains(&root.join("de.po")));
        assert!(!candidates.contains(&root.join("README.md")));
        assert!(!candidates.contains(&root.join("Makefile")));
    }
}
