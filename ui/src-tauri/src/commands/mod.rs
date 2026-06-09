//! Tauri command bindings, organized by feature bucket.
//!
//! `build_handler` registers every command on the provided Tauri
//! `Builder`. The two cfg arms exist because the ollama-gated
//! commands compile out under `--no-default-features`.

pub(crate) mod corrections;
pub(crate) mod eval;
pub(crate) mod file_catalog;
pub(crate) mod glossary;
pub(crate) mod meta;
pub mod project_catalog;
pub mod project_lifecycle;
pub(crate) mod reuse;
pub mod review;
pub(crate) mod settings;
pub(crate) mod translate;
pub(crate) mod tuning;
pub(crate) mod util;

#[cfg(feature = "ollama")]
pub(crate) fn build_handler(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.invoke_handler(tauri::generate_handler![
        meta::app_version,
        file_catalog::open_catalog,
        file_catalog::update_unit_target,
        file_catalog::save_catalog,
        file_catalog::discard_changes,
        file_catalog::translate_unit,
        meta::list_locales,
        glossary::load_glossary,
        glossary::save_glossary,
        glossary::load_metrics,
        project_lifecycle::open_project,
        project_lifecycle::discover_project,
        project_lifecycle::create_project,
        project_lifecycle::close_project,
        project_lifecycle::current_project_summary,
        project_lifecycle::list_catalogs,
        project_lifecycle::save_manifest,
        project_catalog::open_catalog_in_project,
        project_catalog::update_unit_target_in_project,
        project_catalog::save_catalog_in_project,
        project_catalog::save_all_dirty,
        project_catalog::discard_changes_in_project,
        project_catalog::list_open_catalogs,
        project_catalog::is_catalog_dirty,
        translate::translate_unit_in_project,
        translate::translate_glossary_term,
        translate::translate_batch_in_project,
        translate::cancel_translation,
        corrections::record_correction_in_project,
        corrections::list_corrections_in_project,
        corrections::promote_correction_to_curated,
        corrections::un_curate_correction,
        corrections::list_curated_in_project,
        review::set_review_status_in_project,
        review::accept_unit_in_project,
        settings::add_catalog_to_project,
        settings::remove_catalog_from_project,
        settings::update_locale_in_project,
        settings::remove_locale_from_project,
        settings::set_backend_in_project,
        settings::set_glossary_in_project,
        settings::set_prompts_in_project,
        review::scan_project_review_state,
        reuse::reuse_references_in_project,
        reuse::split_remainder,
        reuse::merge_catalogs,
        reuse::add_project_reference,
        reuse::remove_project_reference,
        eval::run_evaluation_in_project,
        eval::list_evaluation_runs_in_project,
        tuning::export_tuning_bundle_in_project,
        tuning::list_tuning_bundles_in_project,
        meta::write_text_file,
    ])
}

#[cfg(not(feature = "ollama"))]
pub(crate) fn build_handler(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.invoke_handler(tauri::generate_handler![
        meta::app_version,
        file_catalog::open_catalog,
        file_catalog::update_unit_target,
        file_catalog::save_catalog,
        file_catalog::discard_changes,
        meta::list_locales,
        glossary::load_glossary,
        glossary::save_glossary,
        glossary::load_metrics,
        project_lifecycle::open_project,
        project_lifecycle::discover_project,
        project_lifecycle::create_project,
        project_lifecycle::close_project,
        project_lifecycle::current_project_summary,
        project_lifecycle::list_catalogs,
        project_lifecycle::save_manifest,
        project_catalog::open_catalog_in_project,
        project_catalog::update_unit_target_in_project,
        project_catalog::save_catalog_in_project,
        project_catalog::save_all_dirty,
        project_catalog::discard_changes_in_project,
        project_catalog::list_open_catalogs,
        project_catalog::is_catalog_dirty,
        translate::cancel_translation,
        corrections::record_correction_in_project,
        corrections::list_corrections_in_project,
        corrections::promote_correction_to_curated,
        corrections::un_curate_correction,
        corrections::list_curated_in_project,
        review::set_review_status_in_project,
        review::accept_unit_in_project,
        settings::add_catalog_to_project,
        settings::remove_catalog_from_project,
        settings::update_locale_in_project,
        settings::remove_locale_from_project,
        settings::set_backend_in_project,
        settings::set_glossary_in_project,
        settings::set_prompts_in_project,
        review::scan_project_review_state,
        reuse::reuse_references_in_project,
        reuse::split_remainder,
        reuse::merge_catalogs,
        reuse::add_project_reference,
        reuse::remove_project_reference,
        eval::list_evaluation_runs_in_project,
        tuning::export_tuning_bundle_in_project,
        tuning::list_tuning_bundles_in_project,
        meta::write_text_file,
    ])
}
