//! Tests for `Project::review_map()` cache invalidation.
//!
//! Covers:
//! - `review_map()` → `set_review_status()` → `review_map()` reflects the new
//!   value (cache is invalidated by each successful write).
//! - Multiple status changes all reflect in subsequent `review_map()` calls.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use i18n_harness_core::{ReviewStatus, UnitId};
use i18n_harness_project::{InMemoryFs, Project, ProjectFs};

const ROOT: &str = "/proj";

fn root() -> &'static Path {
    Path::new(ROOT)
}

const MANIFEST: &str = r#"[project]
name = "cache-invalidation-test"
schema = 1

[[catalogs]]
path = "translations/app_de.ts"
format = "qt-ts"
locale = "de_DE"
"#;

fn open_project() -> Project {
    let raw = Arc::new(InMemoryFs::new());
    let root_path = PathBuf::from(ROOT);
    raw.write_atomic(&root_path.join("i18n-harness.toml"), MANIFEST.as_bytes())
        .unwrap();
    raw.create_dir_all(&root_path.join("translations")).unwrap();
    raw.write_atomic(&root_path.join("translations/app_de.ts"), b"<TS></TS>")
        .unwrap();

    let fs: Arc<dyn ProjectFs> = raw;
    let (project, _) = Project::open_with_fs(root(), fs).expect("open");
    project
}

#[test]
fn new_status_visible_after_cache_invalidation() {
    let project = open_project();
    let catalog = Path::new("translations/app_de.ts");
    let unit_id = UnitId::from("Ctx::Hello");

    // First call populates the cache (empty).
    {
        let map = project.review_map();
        assert!(map.is_empty(), "initially empty");
    }

    // Write a status → must invalidate cache.
    project
        .set_review_status(
            catalog,
            &unit_id,
            Some(ReviewStatus::Approved),
            "sha256:aabbcc001122".to_owned(),
            None,
        )
        .expect("set_review_status");

    // Next call must re-fold and return the new value.
    {
        let map = project.review_map();
        assert_eq!(map.len(), 1, "map must show the new record");
        let key = (PathBuf::from("translations/app_de.ts"), unit_id.clone());
        assert!(map.contains_key(&key), "key must be in map");
    }
}

#[test]
fn status_update_replaces_prior_in_cache() {
    let project = open_project();
    let catalog = Path::new("translations/app_de.ts");
    let unit_id = UnitId::from("Ctx::Hello");

    project
        .set_review_status(
            catalog,
            &unit_id,
            Some(ReviewStatus::MachineTranslated),
            "sha256:111".to_owned(),
            None,
        )
        .expect("first set");

    project
        .set_review_status(
            catalog,
            &unit_id,
            Some(ReviewStatus::Approved),
            "sha256:222".to_owned(),
            None,
        )
        .expect("second set");

    let map = project.review_map();
    let key = (PathBuf::from("translations/app_de.ts"), unit_id.clone());
    let record = map.get(&key).expect("key must be present");
    assert_eq!(record.status, ReviewStatus::Approved, "last write wins");
    assert_eq!(record.source_hash_at_review, "sha256:222");
}

#[test]
fn clear_event_removes_entry_from_cache() {
    let project = open_project();
    let catalog = Path::new("translations/app_de.ts");
    let unit_id = UnitId::from("Ctx::Hello");

    project
        .set_review_status(
            catalog,
            &unit_id,
            Some(ReviewStatus::Approved),
            "sha256:abc".to_owned(),
            None,
        )
        .expect("set");

    {
        let map = project.review_map();
        assert_eq!(map.len(), 1);
    }

    project
        .set_review_status(catalog, &unit_id, None, String::new(), None)
        .expect("clear");

    {
        let map = project.review_map();
        assert!(map.is_empty(), "clear event must remove the entry");
    }
}
