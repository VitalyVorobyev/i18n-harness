//! Pure path helpers shared by multiple project-mode commands.
//!
//! These are free functions with no locks or I/O — they operate purely on
//! in-memory project data and string slices.

use std::path::PathBuf;

use i18n_harness_project::{CorrectionId, Project};

/// Return `(absolute_path, manifest_relative_path)` for a catalog path that
/// may be absolute or manifest-relative. Errors with
/// `"catalog not registered in project"` if no registered catalog matches.
pub(crate) fn resolve_catalog_path(
    project: &Project,
    input: &str,
) -> Result<(PathBuf, PathBuf), String> {
    let input_path = PathBuf::from(input);
    let catalog_ref = project
        .catalog(&input_path)
        .ok_or_else(|| "catalog not registered in project".to_string())?;
    let absolute = PathBuf::from(&catalog_ref.absolute_path);
    let manifest_relative = PathBuf::from(&catalog_ref.manifest_path);
    Ok((absolute, manifest_relative))
}

/// Parse a `"corr_<12-hex>"` string into a [`CorrectionId`].
///
/// Returns a generic `"invalid correction id"` on failure to avoid leaking
/// internal format details to the caller.
pub(crate) fn parse_correction_id(s: &str) -> Result<CorrectionId, String> {
    if s.starts_with("corr_") && s.len() == 17 && s[5..].chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(CorrectionId(s.to_owned()))
    } else {
        Err("invalid correction id".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_correction_id_accepts_valid_prefix_and_hex() {
        let id = parse_correction_id("corr_0123456789ab").unwrap();
        assert_eq!(id.0, "corr_0123456789ab");
    }

    #[test]
    fn parse_correction_id_rejects_wrong_length() {
        // Too short: only 11 hex chars instead of 12.
        assert!(parse_correction_id("corr_01234567890").is_err());
        // Too long: 13 hex chars.
        assert!(parse_correction_id("corr_0123456789abc").is_err());
        // Empty after prefix.
        assert!(parse_correction_id("corr_").is_err());
    }

    #[test]
    fn parse_correction_id_rejects_non_hex() {
        // 'g' is not a hex digit.
        assert!(parse_correction_id("corr_0123456789ag").is_err());
        // Upper-case G.
        assert!(parse_correction_id("corr_0123456789aG").is_err());
        // Space in the hex part.
        assert!(parse_correction_id("corr_01234 6789ab").is_err());
    }

    #[test]
    fn parse_correction_id_rejects_wrong_prefix() {
        assert!(parse_correction_id("correction_0123456789ab").is_err());
        assert!(parse_correction_id("0123456789ab").is_err());
        assert!(parse_correction_id("").is_err());
    }
}
