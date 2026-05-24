//! Tests for `ProjectPaths::review()`.
//!
//! Covers:
//! - Default state_dir → `<root>/.i18n-harness/review.jsonl`.
//! - `[paths] state_dir` override → correct path.

use std::path::{Path, PathBuf};

use i18n_harness_project::{PathsConfig, ProjectPaths};

fn default_paths() -> ProjectPaths {
    ProjectPaths::new(
        Path::new("/my/project"),
        &PathsConfig::default(),
        None,
        None,
    )
}

#[test]
fn review_path_is_in_default_state_dir() {
    let paths = default_paths();
    let expected = PathBuf::from("/my/project/.i18n-harness/review.jsonl");
    assert_eq!(paths.review(), expected.as_path());
}

#[test]
fn review_path_respects_state_dir_override() {
    let config = PathsConfig {
        state_dir: Some(PathBuf::from("/custom/state")),
    };
    let paths = ProjectPaths::new(Path::new("/my/project"), &config, None, None);
    let expected = PathBuf::from("/custom/state/review.jsonl");
    assert_eq!(paths.review(), expected.as_path());
}

#[test]
fn review_path_respects_relative_state_dir_override() {
    let config = PathsConfig {
        state_dir: Some(PathBuf::from("my-state")),
    };
    let paths = ProjectPaths::new(Path::new("/my/project"), &config, None, None);
    let expected = PathBuf::from("/my/project/my-state/review.jsonl");
    assert_eq!(paths.review(), expected.as_path());
}
