//! Catalog format detection for discovery and manifest validation.
//!
//! The format-specific sniffers live with their formats; this module asks the
//! format registry and turns its answer into a [`DraftCatalog`].

use std::path::Path;

use i18n_harness_formats::{CatalogFormat, Confidence, FormatRegistry};

use super::{ClassificationConfidence, DraftCatalog, FormatGuess};
use crate::discovery::locale_infer;
use crate::manifest::FormatId;

/// Classify the file at `path` whose first bytes are `bytes`.
///
/// Uses the best-ranked registry match, falls back to filename-based locale
/// inference when the content records no language, and produces
/// [`FormatGuess::Unknown`] when nothing matched.
pub(crate) fn sniff(path: &Path, bytes: &[u8]) -> DraftCatalog {
    let registry = FormatRegistry::builtin();
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    if !registry.claims_extension(path) {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        return unknown(
            path,
            format!("extension `.{ext}` is not a known catalog extension"),
        );
    }

    let Some(best) = registry.sniff_bytes(path, bytes).into_iter().next() else {
        let names: Vec<_> = registry
            .formats()
            .iter()
            .filter(|f| claims(**f, path))
            .map(|f| f.display_name())
            .collect();
        return unknown(path, format!("content is not {}", names.join(" or ")));
    };

    let content_locale = best.hit.language.is_some();
    let locale = best
        .hit
        .language
        .or_else(|| locale_infer::infer_from_filename(filename));
    let mut confidence = classification(best.hit.confidence);
    // A confident format without any locale evidence is one level less sure.
    if !content_locale && locale.is_none() && confidence == ClassificationConfidence::High {
        confidence = ClassificationConfidence::Medium;
    }
    DraftCatalog {
        path: path.to_path_buf(),
        format: FormatGuess::Known(FormatId::new(best.format.id())),
        locale,
        confidence,
        reason: best.hit.reason,
        alternatives: vec![],
    }
}

/// Whether `bytes` are consistent with the declared `format`.
///
/// Used by `Project::open` to catch a manifest that names the wrong format.
/// An id the registry does not know never confirms.
pub(crate) fn confirms_format(path: &Path, bytes: &[u8], declared: &FormatId) -> bool {
    FormatRegistry::builtin()
        .get(declared.as_str())
        .is_some_and(|format| format.sniff(path, bytes).is_some())
}

/// The format the content looks like, regardless of its extension (used to
/// populate `sniffed` in format-mismatch errors).
pub(crate) fn guess_format(path: &Path, bytes: &[u8]) -> FormatGuess {
    FormatRegistry::builtin()
        .formats()
        .iter()
        .filter_map(|f| f.sniff(path, bytes).map(|hit| (f.id(), hit.confidence)))
        .max_by_key(|(_, confidence)| *confidence)
        .map_or(FormatGuess::Unknown, |(id, _)| {
            FormatGuess::Known(FormatId::new(id))
        })
}

fn claims(format: &dyn CatalogFormat, path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    format
        .extensions()
        .iter()
        .any(|e| e.eq_ignore_ascii_case(ext))
}

fn classification(confidence: Confidence) -> ClassificationConfidence {
    match confidence {
        Confidence::High => ClassificationConfidence::High,
        Confidence::Medium => ClassificationConfidence::Medium,
        Confidence::Low => ClassificationConfidence::Low,
    }
}

fn unknown(path: &Path, reason: String) -> DraftCatalog {
    DraftCatalog {
        path: path.to_path_buf(),
        format: FormatGuess::Unknown,
        locale: None,
        confidence: ClassificationConfidence::Low,
        reason,
        alternatives: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const QT: &[u8] = br#"<?xml version="1.0"?><TS version="2.1" language="de_DE"></TS>"#;
    const PO: &[u8] = b"msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain; charset=UTF-8\\n\"\n";

    #[test]
    fn qt_file_with_language_is_high() {
        let dc = sniff(Path::new("app.ts"), QT);
        assert_eq!(dc.format, FormatGuess::Known("qt-ts".into()));
        assert_eq!(dc.locale.as_deref(), Some("de_DE"));
        assert_eq!(dc.confidence, ClassificationConfidence::High);
    }

    #[test]
    fn typescript_source_is_unknown() {
        let dc = sniff(Path::new("main.ts"), b"export const x = 1;\n");
        assert_eq!(dc.format, FormatGuess::Unknown);
        assert!(dc.reason.contains("Qt Linguist"), "{}", dc.reason);
    }

    #[test]
    fn locale_falls_back_to_the_filename() {
        let dc = sniff(Path::new("messages_fr_FR.po"), PO);
        assert_eq!(dc.format, FormatGuess::Known("gettext-po".into()));
        assert_eq!(dc.locale.as_deref(), Some("fr_FR"));
    }

    #[test]
    fn unknown_extension_is_unknown() {
        let dc = sniff(Path::new("readme.md"), b"# hi");
        assert_eq!(dc.format, FormatGuess::Unknown);
        assert!(dc.reason.contains(".md"));
    }

    #[test]
    fn confirms_the_declared_format() {
        let p = Path::new("x");
        assert!(confirms_format(p, QT, &"qt-ts".into()));
        assert!(!confirms_format(p, b"{\"key\": \"val\"}", &"qt-ts".into()));
        assert!(confirms_format(p, PO, &"gettext-po".into()));
        assert!(!confirms_format(p, QT, &"gettext-po".into()));
        assert!(confirms_format(p, b"{\"k\": \"v\"}", &"icu-json".into()));
        assert!(!confirms_format(p, QT, &"xliff".into()));
    }

    #[test]
    fn guess_ignores_the_extension() {
        assert_eq!(
            guess_format(Path::new("x.json"), QT),
            FormatGuess::Known("qt-ts".into())
        );
        assert_eq!(guess_format(Path::new("x"), b"plain"), FormatGuess::Unknown);
    }

    #[test]
    fn guess_format_serializes_as_id_or_unknown() {
        let known = serde_json::to_string(&FormatGuess::Known("qt-ts".into())).unwrap();
        assert_eq!(known, "\"qt-ts\"");
        assert_eq!(
            serde_json::to_string(&FormatGuess::Unknown).unwrap(),
            "\"unknown\""
        );
    }
}
