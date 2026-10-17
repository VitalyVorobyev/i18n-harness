//! The registry of built-in catalog formats.
//!
//! Every caller that needs to recognize or open a catalog file — project
//! discovery, the CLI, the desktop shell, the translation session — goes
//! through [`FormatRegistry`]. Adding a format means implementing
//! [`CatalogFormat`] in its own crate or module and listing it in
//! [`FormatRegistry::builtin`]; nothing else names formats.

#![forbid(unsafe_code)]

use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;

pub use i18n_harness_catalog::{
    CatalogError, CatalogFormat, Confidence, FormatDoc, PlaceholderError, SniffHit,
};

/// How many bytes of a file sniffing reads.
pub const SNIFF_HEAD_BYTES: usize = 64 * 1024;

/// One format that recognized a file.
#[derive(Debug, Clone)]
pub struct Detection {
    /// The format that matched.
    pub format: &'static dyn CatalogFormat,
    /// What the format reported.
    pub hit: SniffHit,
}

/// An ordered set of catalog formats.
#[derive(Debug)]
pub struct FormatRegistry {
    formats: Vec<&'static dyn CatalogFormat>,
}

impl FormatRegistry {
    /// Build a registry from an explicit format list (registry order breaks
    /// ties between equally confident sniffs).
    pub fn new(formats: Vec<&'static dyn CatalogFormat>) -> Self {
        Self { formats }
    }

    /// Every format that ships with the harness.
    pub fn builtin() -> &'static Self {
        static REGISTRY: OnceLock<FormatRegistry> = OnceLock::new();
        REGISTRY.get_or_init(|| {
            let mut formats: Vec<&'static dyn CatalogFormat> =
                vec![&i18n_harness_adapter_qt::QT_TS_FORMAT];
            formats.extend(i18n_harness_catalog::builtin_formats());
            Self::new(formats)
        })
    }

    /// The formats, in registry order.
    pub fn formats(&self) -> &[&'static dyn CatalogFormat] {
        &self.formats
    }

    /// The format with manifest id `id`.
    pub fn get(&self, id: &str) -> Option<&'static dyn CatalogFormat> {
        self.formats.iter().copied().find(|f| f.id() == id)
    }

    /// Whether some format claims `path`'s extension.
    pub fn claims_extension(&self, path: &Path) -> bool {
        !self.formats_for_extension(path).is_empty()
    }

    /// Sniff already-read bytes. Only formats claiming `path`'s extension
    /// are consulted. Results are ranked by confidence, then registry order.
    pub fn sniff_bytes(&self, path: &Path, head: &[u8]) -> Vec<Detection> {
        let mut found: Vec<Detection> = self
            .formats_for_extension(path)
            .into_iter()
            .filter_map(|format| {
                format
                    .sniff(path, head)
                    .map(|hit| Detection { format, hit })
            })
            .collect();
        // Stable sort keeps registry order among equal confidences.
        found.sort_by_key(|d| std::cmp::Reverse(d.hit.confidence));
        found
    }

    /// Read the head of `path` and sniff it (see [`Self::sniff_bytes`]).
    ///
    /// # Errors
    ///
    /// [`CatalogError::Io`] when the file cannot be read.
    pub fn sniff(&self, path: &Path) -> Result<Vec<Detection>, CatalogError> {
        let head = read_head(path)?;
        Ok(self.sniff_bytes(path, &head))
    }

    /// Open `path` with the format named `format`, or with the best sniffed
    /// format when `format` is `None`.
    ///
    /// # Errors
    ///
    /// - [`CatalogError::UnsupportedFormat`] for an unknown id, or when no
    ///   format recognizes the file.
    /// - Any error from reading or parsing the file.
    pub fn open(
        &self,
        path: &Path,
        format: Option<&str>,
    ) -> Result<Box<dyn FormatDoc>, CatalogError> {
        let format = match format {
            Some(id) => self
                .get(id)
                .ok_or_else(|| CatalogError::UnsupportedFormat(id.to_owned()))?,
            None => {
                self.sniff(path)?
                    .into_iter()
                    .next()
                    .ok_or_else(|| {
                        CatalogError::UnsupportedFormat(format!(
                            "{} is not a recognized catalog file",
                            path.display()
                        ))
                    })?
                    .format
            }
        };
        format.open(path)
    }

    fn formats_for_extension(&self, path: &Path) -> Vec<&'static dyn CatalogFormat> {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            return Vec::new();
        };
        self.formats
            .iter()
            .copied()
            .filter(|f| f.extensions().iter().any(|e| e.eq_ignore_ascii_case(ext)))
            .collect()
    }
}

/// Read at most [`SNIFF_HEAD_BYTES`] from the start of `path`.
///
/// # Errors
///
/// [`CatalogError::Io`] when the file cannot be opened or read.
pub fn read_head(path: &Path) -> Result<Vec<u8>, CatalogError> {
    let io_err = |source| CatalogError::Io {
        op: "read",
        path: path.to_path_buf(),
        source,
    };
    let file = std::fs::File::open(path).map_err(io_err)?;
    let mut head = Vec::with_capacity(SNIFF_HEAD_BYTES);
    file.take(SNIFF_HEAD_BYTES as u64)
        .read_to_end(&mut head)
        .map_err(io_err)?;
    Ok(head)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn fixture(rel: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(rel)
    }

    #[test]
    fn builtin_registry_lists_every_format_once() {
        let ids: Vec<_> = FormatRegistry::builtin()
            .formats()
            .iter()
            .map(|f| f.id())
            .collect();
        assert_eq!(
            ids,
            [
                "qt-ts",
                "lingui-po",
                "gettext-po",
                "lingui-json",
                "icu-json"
            ]
        );
    }

    #[test]
    fn sniff_recognizes_each_fixture_family() {
        let reg = FormatRegistry::builtin();
        for (rel, id) in [
            ("qt/sk_SK_pivot.ts", "qt-ts"),
            ("po/plurals_polish.po", "gettext-po"),
            ("icu-json/flat.json", "icu-json"),
        ] {
            let hits = reg.sniff(&fixture(rel)).expect("read fixture");
            assert_eq!(hits.first().map(|d| d.format.id()), Some(id), "{rel}");
        }
    }

    #[test]
    fn typescript_source_is_not_a_catalog() {
        let hits =
            FormatRegistry::builtin().sniff_bytes(Path::new("app.ts"), b"export const x = 1;\n");
        assert!(hits.is_empty());
    }

    #[test]
    fn open_without_format_uses_the_sniffed_one() {
        let doc = FormatRegistry::builtin()
            .open(&fixture("qt/sk_SK_pivot.ts"), None)
            .expect("open");
        assert_eq!(doc.format().id(), "qt-ts");
        assert_eq!(doc.language(), Some("sk_SK"));
    }

    #[test]
    fn open_with_unknown_format_id_is_an_error() {
        let err = FormatRegistry::builtin()
            .open(&fixture("qt/sk_SK_pivot.ts"), Some("xliff"))
            .unwrap_err();
        assert!(matches!(err, CatalogError::UnsupportedFormat(id) if id == "xliff"));
    }

    #[test]
    fn unclaimed_extension_is_not_sniffed() {
        assert!(!FormatRegistry::builtin().claims_extension(Path::new("notes.txt")));
        assert!(
            FormatRegistry::builtin()
                .sniff_bytes(Path::new("notes.txt"), b"msgid \"a\"\nmsgstr \"b\"\n")
                .is_empty()
        );
    }
}
