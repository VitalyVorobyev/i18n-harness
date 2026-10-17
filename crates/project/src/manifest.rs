//! Pure-serde read view of `i18n-harness.toml`.
//!
//! All types here are constructed by parsing TOML text; none are mutated
//! in place. Mutation of the on-disk manifest goes through `Project`'s
//! `toml_edit`-backed methods (slice b). The types here are the *query
//! layer* — callers read fields from `ProjectManifest` and write fields
//! through `Project::set_*` / `Project::add_*`.
//!
//! See `docs/m4.1-project-crate-design.md` §1.2 and §2.4 for the
//! `deny_unknown_fields` strategy: sub-tables use it (typos are the common
//! failure mode there) but the root manifest does not (so v2-only top-level
//! tables do not break a v1 binary reading a v2 file).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{ProjectError, ProjectWarning};
use crate::fs::ProjectFs;

/// Current schema version this binary understands.
///
/// The loader rejects manifests whose `[project] schema` exceeds this value
/// and accepts lower values as forward-compatible reads.
pub const SCHEMA_VERSION: u32 = 1;

/// Pure-serde read view of `i18n-harness.toml`.
///
/// Construct via [`ProjectManifest::from_toml`] or [`ProjectManifest::load`];
/// never edit this in place to mutate the file — go through `Project::set_*`
/// / `Project::add_*`, which preserve comments and ordering via `toml_edit`.
///
/// The root struct intentionally does **not** use `deny_unknown_fields` so
/// that a binary built against schema v1 can open a schema v2 file that adds
/// new top-level tables without hard-failing. Sub-tables use
/// `deny_unknown_fields` because typos in their known fields are the common
/// failure mode and new v2 fields land in new tables, not new sub-keys of
/// existing ones.
///
/// `Eq` is not derived because [`LocaleConfig`] contains `f32`, which does
/// not implement `Eq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectManifest {
    /// `[project]` table — name and schema version.
    pub project: ProjectMeta,
    /// `[locales.<id>]` tables — per-locale config overrides.
    #[serde(default)]
    pub locales: BTreeMap<String, LocaleConfig>,
    /// `[[catalogs]]` array — registered catalog files.
    #[serde(default)]
    pub catalogs: Vec<CatalogEntry>,
    /// `[[references]]` array — expert-translated catalogs that may be used as
    /// translation-memory sources. Absent in older manifests; defaults to empty.
    #[serde(default)]
    pub references: Vec<ReferenceEntry>,
    /// `[glossary]` table — optional glossary config.
    pub glossary: Option<GlossaryConfig>,
    /// `[backend]` table — backend selection and options.
    #[serde(default, rename = "backend")]
    pub backends: BackendBlock,
    /// `[prompts]` table — prompt template directory, if any.
    pub prompts: Option<PromptsConfig>,
    /// `[paths]` table — optional state-path overrides.
    #[serde(default)]
    pub paths: PathsConfig,
}

/// `[project]` sub-table — name and schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectMeta {
    /// Human-readable project name (used in UI titles and prompt context).
    pub name: String,
    /// Schema version declared by this manifest. Must be `<= SCHEMA_VERSION`.
    pub schema: u32,
}

/// `[locales.<id>]` sub-table — per-locale overrides layered on top of the
/// workspace locale record.
///
/// All fields are optional: an absent field means "use the workspace default".
/// See `docs/m4.1-project-crate-design.md` §4 for the full layering rule.
///
/// `Eq` is intentionally omitted: `length_warn_ratio` is `f32`, which does
/// not implement `Eq`. Use `PartialEq` comparisons at callsites.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocaleConfig {
    /// Override the workspace register (formal / informal / neutral).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub register: Option<RegisterOverride>,
    /// Override the workspace variant tag (e.g. `"es_419"` for Latin American
    /// Spanish within a manifest that defaults to `"es_ES"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    /// Override the workspace length-warn ratio. `1.3` means warn when the
    /// translation is 30 % longer than the source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length_warn_ratio: Option<f32>,
}

/// Register style for a locale — how formal the translated text should be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RegisterOverride {
    /// Formal register (Sie, usted, vous …).
    Formal,
    /// Informal register (du, tú, tu …).
    Informal,
    /// Neutral / undifferentiated register.
    Neutral,
}

/// One entry in the `[[catalogs]]` array — a registered translation file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogEntry {
    /// Path to the catalog file, relative to the project root.
    pub path: PathBuf,
    /// Declared format of the catalog.
    pub format: CatalogFormat,
    /// Locale id this catalog serves (e.g. `"de_DE"`).
    pub locale: String,
}

/// One entry in the `[[references]]` array — an expert-translated catalog
/// whose translations may be reused into other catalogs of the same locale.
///
/// References are read-only at runtime; the harness never writes back to them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceEntry {
    /// Path to the reference catalog file, relative to the project root.
    pub path: PathBuf,
    /// Declared format of the reference catalog.
    pub format: CatalogFormat,
    /// Locale id this reference serves (e.g. `"de_DE"`).
    pub locale: String,
}

/// Catalog file format discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CatalogFormat {
    /// Qt Linguist `.ts` XML format.
    QtTs,
    /// GNU gettext `.po` / `.pot` format.
    GettextPo,
    /// ICU MessageFormat JSON (flat key → message map).
    IcuJson,
}

/// `[glossary]` sub-table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlossaryConfig {
    /// Path to `glossary.toml`, relative to the project root.
    pub path: PathBuf,
}

/// `[backend]` table wrapper. Holds `default` and (in future slices) named
/// per-locale overrides.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendBlock {
    /// Default backend used when no per-locale override is set.
    pub default: Option<BackendConfig>,
}

/// Backend selection and connection parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendConfig {
    /// Which backend implementation to use.
    pub kind: BackendKind,
    /// Model identifier (e.g. `"gemma3:27b"`). Not all backends use this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// HTTP host for network-based backends (e.g. `"http://localhost:11434"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// Context window size in tokens. Overrides the backend's default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_ctx: Option<u32>,
}

/// Discriminator for the backend implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendKind {
    /// Human enters translations manually — no model involved.
    Manual,
    /// Ollama local inference server.
    Ollama,
    /// Any OpenAI-compatible HTTP API (OpenAI, LM Studio, …).
    OpenAiCompatible,
    /// Anthropic managed agent / Claude API.
    Agent,
}

/// `[prompts]` sub-table — location of prompt template files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptsConfig {
    /// Directory that holds per-locale prompt template files, relative to the
    /// project root.
    pub template_dir: PathBuf,
}

/// `[paths]` sub-table — optional overrides for state file locations.
///
/// Absent fields fall through to the in-tree defaults
/// (`<project>/.i18n-harness/<name>`). See `docs/m4.1-project-crate-design.md`
/// §7 for the full path-resolution policy.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathsConfig {
    /// Override the state directory. Relative paths resolve against the
    /// manifest's directory; absolute paths are used as-is.
    pub state_dir: Option<PathBuf>,
}

/// Format guess produced by the discovery heuristic.
///
/// Defined here (rather than in a future `discovery` module) so that
/// [`crate::error::ProjectError::CatalogFormatMismatch`] can reference it
/// without a forward dependency on the discovery module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FormatGuess {
    /// Content sniff identified Qt Linguist XML.
    QtTs,
    /// Content sniff identified GNU gettext PO.
    GettextPo,
    /// Content sniff identified ICU MessageFormat JSON.
    IcuJson,
    /// File looked plausible but neither sniff nor extension confirmed a
    /// format. UI shows it greyed-out with "skip" preselected.
    Unknown,
}

impl ProjectManifest {
    /// Parse and validate a manifest from a TOML string.
    ///
    /// Returns the parsed manifest and an empty warnings vector (warnings that
    /// require locale resolution populate in later slices once the workspace
    /// `Locale::by_id` check lives in the project crate).
    ///
    /// # Errors
    ///
    /// - [`ProjectError::MissingRequiredField`] if `[project] schema` is absent.
    /// - [`ProjectError::UnsupportedSchemaVersion`] if `schema > SCHEMA_VERSION`.
    /// - [`ProjectError::ManifestParse`] if the TOML is malformed or a
    ///   sub-table contains an unknown field.
    pub fn from_toml(s: &str) -> Result<(Self, Vec<ProjectWarning>), ProjectError> {
        // Parse through a raw intermediate that captures the optional schema
        // field so we can give a precise error when it is missing.
        let raw: RawManifest = toml::from_str(s).map_err(|source| ProjectError::ManifestParse {
            path: PathBuf::from("<input>"),
            source,
        })?;

        let schema = raw
            .project
            .schema
            .ok_or_else(|| ProjectError::MissingRequiredField {
                field: "project.schema".into(),
            })?;

        if schema == 0 || schema > SCHEMA_VERSION {
            return Err(ProjectError::UnsupportedSchemaVersion {
                found: schema,
                supported: SCHEMA_VERSION,
            });
        }

        // Re-parse into the fully-typed manifest now that we know the schema
        // is acceptable.
        let manifest: ProjectManifest =
            toml::from_str(s).map_err(|source| ProjectError::ManifestParse {
                path: PathBuf::from("<input>"),
                source,
            })?;

        Ok((manifest, vec![]))
    }

    /// Read `path` via `fs` and parse it as a manifest.
    ///
    /// Combines filesystem I/O with [`Self::from_toml`]; errors from the read
    /// surface as [`ProjectError::Io`]; parse errors use the real `path` in
    /// their diagnostic.
    pub fn load(
        path: &Path,
        fs: &dyn ProjectFs,
    ) -> Result<(Self, Vec<ProjectWarning>), ProjectError> {
        let text = fs.read_to_string(path).map_err(|source| ProjectError::Io {
            path: path.to_path_buf(),
            source,
        })?;

        // Parse once to get a typed result; remap the generic `<input>` path
        // to the real path so diagnostic messages name the file.
        Self::from_toml(&text).map_err(|e| match e {
            ProjectError::ManifestParse {
                path: ref p,
                source,
            } if p == Path::new("<input>") => ProjectError::ManifestParse {
                path: path.to_path_buf(),
                source,
            },
            other => other,
        })
    }

    /// Schema version declared by this manifest.
    pub fn schema_version(&self) -> u32 {
        self.project.schema
    }

    /// Default backend config, if one is declared under `[backend.default]`.
    pub fn default_backend(&self) -> Option<&BackendConfig> {
        self.backends.default.as_ref()
    }
}

// ── Private raw intermediate ─────────────────────────────────────────────────

/// Raw intermediate used only during `from_toml` to detect a missing
/// `project.schema` before the typed parse.
///
/// `schema` is `Option<u32>` here so we can distinguish "field absent"
/// (→ `MissingRequiredField`) from "field present but wrong type"
/// (→ `ManifestParse`).
#[derive(Deserialize)]
struct RawManifest {
    project: RawProjectMeta,
}

#[derive(Deserialize)]
struct RawProjectMeta {
    schema: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_version_constant_is_one() {
        assert_eq!(SCHEMA_VERSION, 1);
    }

    #[test]
    fn schema_zero_is_rejected() {
        let toml = r#"
[project]
name = "test"
schema = 0
"#;
        let err = ProjectManifest::from_toml(toml).unwrap_err();
        assert!(
            matches!(err, ProjectError::UnsupportedSchemaVersion { found: 0, .. }),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn schema_too_new_is_rejected() {
        let toml = r#"
[project]
name = "test"
schema = 999
"#;
        let err = ProjectManifest::from_toml(toml).unwrap_err();
        assert!(
            matches!(
                err,
                ProjectError::UnsupportedSchemaVersion { found: 999, .. }
            ),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn missing_schema_field_gives_precise_error() {
        let toml = r#"
[project]
name = "test"
"#;
        let err = ProjectManifest::from_toml(toml).unwrap_err();
        assert!(
            matches!(err, ProjectError::MissingRequiredField { ref field } if field == "project.schema"),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn references_round_trips_through_from_toml() {
        let toml = r#"
[project]
name = "test"
schema = 1

[[references]]
path = "expert/app_de.ts"
format = "qt-ts"
locale = "de_DE"

[[references]]
path = "expert/app_es.po"
format = "gettext-po"
locale = "es_ES"
"#;
        let (manifest, _) = ProjectManifest::from_toml(toml).expect("parse");
        assert_eq!(manifest.references.len(), 2);
        assert_eq!(
            manifest.references[0].path,
            PathBuf::from("expert/app_de.ts")
        );
        assert_eq!(manifest.references[0].format, CatalogFormat::QtTs);
        assert_eq!(manifest.references[0].locale, "de_DE");
        assert_eq!(manifest.references[1].format, CatalogFormat::GettextPo);
    }

    #[test]
    fn manifest_without_references_defaults_to_empty() {
        let toml = r#"
[project]
name = "test"
schema = 1

[[catalogs]]
path = "translations/app_de.ts"
format = "qt-ts"
locale = "de_DE"
"#;
        let (manifest, _) = ProjectManifest::from_toml(toml).expect("parse");
        assert!(
            manifest.references.is_empty(),
            "references should default to empty when absent"
        );
    }

    #[test]
    fn old_style_manifest_unaffected_by_references_field() {
        // A manifest with no [[references]] section parses cleanly and has
        // catalogs, locales, and other fields intact.
        let toml = r#"
[project]
name = "legacy"
schema = 1

[locales.de_DE]
register = "formal"

[[catalogs]]
path = "app.ts"
format = "qt-ts"
locale = "de_DE"
"#;
        let (manifest, warnings) = ProjectManifest::from_toml(toml).expect("parse");
        assert!(manifest.references.is_empty());
        assert_eq!(manifest.catalogs.len(), 1);
        assert!(manifest.locales.contains_key("de_DE"));
        assert!(warnings.is_empty());
    }
}
