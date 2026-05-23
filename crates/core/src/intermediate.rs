//! [`Intermediate`] — the on-disk JSONL representation of a batch of
//! [`crate::Unit`]s.
//!
//! One line per unit, with a leading `schema_version` field on each line.
//! This is the format used to:
//!
//! - persist batches between runs (resumability),
//! - hand units to an out-of-process agent (`export-batch` / `import-batch`
//!   in M4),
//! - and snapshot a catalog into a textual form a human can `diff`.
//!
//! Per-line schema versioning (rather than file-level) lets a future binary
//! tolerate mixed-version files when migrating; we have no plans to need
//! that, but it costs almost nothing to design in.
//!
//! # What this module guarantees
//!
//! - Lossless round-trip through `serde_json`: parsing a serialized
//!   [`IntermediateLine`] and re-serializing produces the same JSON object
//!   (modulo whitespace, which `serde_json` normalizes).
//! - A binary that does not recognize a `schema_version` returns an explicit
//!   [`crate::IntermediateError::UnsupportedSchemaVersion`] rather than
//!   silently misinterpreting fields.
//!
//! # What it does NOT guarantee
//!
//! - That the catalog the units came from still exists at the recorded
//!   provenance path.
//! - That a unit's `target` is well-formed ICU or satisfies plural arity —
//!   those are gate concerns.

use serde::{Deserialize, Serialize};

use crate::error::IntermediateError;
use crate::unit::Unit;

/// Highest [`IntermediateLine::schema_version`] this binary knows how to
/// read. Bump this on every breaking change to the line shape; never decrease
/// it.
pub const SCHEMA_VERSION: u32 = 1;

/// One line of an [`Intermediate`] file: a versioned envelope around a
/// [`Unit`].
///
/// Carrying the version on every line (rather than a header) lets streaming
/// readers reject unknown versions per-line without buffering the whole file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateLine {
    /// On-disk schema version for this line. See [`SCHEMA_VERSION`].
    pub schema_version: u32,

    /// The unit payload.
    #[serde(flatten)]
    pub unit: Unit,
}

impl IntermediateLine {
    /// Wrap a unit with the current [`SCHEMA_VERSION`].
    pub fn current(unit: Unit) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            unit,
        }
    }
}

/// An in-memory representation of an intermediate JSONL file — an ordered
/// sequence of versioned unit lines.
///
/// Adapters and the CLI use the [`Self::to_jsonl`] / [`Self::from_jsonl`]
/// helpers to round-trip; nothing in the design forces this struct to mirror
/// a single on-disk file (you can build one from any iterator of `Unit`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Intermediate {
    /// Lines in stable order. Adapters typically produce them in
    /// `(file, unit-id)` order; the type does not enforce that.
    pub lines: Vec<IntermediateLine>,
}

impl Intermediate {
    /// Build from a sequence of units, wrapping each in the current schema
    /// version.
    pub fn from_units<I>(units: I) -> Self
    where
        I: IntoIterator<Item = Unit>,
    {
        Self {
            lines: units.into_iter().map(IntermediateLine::current).collect(),
        }
    }

    /// Number of lines.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// True if there are no lines.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Serialize as JSONL (one JSON object per line, trailing newline).
    ///
    /// # Errors
    ///
    /// Returns [`IntermediateError::InvalidJson`] only if `serde_json` fails
    /// to serialize a line — in practice this means a field with a
    /// non-string map key, which `Unit` does not have, so this is
    /// effectively infallible for the current shape. It is still surfaced as
    /// an error so future shape changes do not silently corrupt files.
    pub fn to_jsonl(&self) -> Result<String, IntermediateError> {
        let mut out = String::new();
        for line in &self.lines {
            let s = serde_json::to_string(line)?;
            out.push_str(&s);
            out.push('\n');
        }
        Ok(out)
    }

    /// Parse a JSONL string back into [`Intermediate`].
    ///
    /// Blank lines (only) are skipped, so a trailing newline does not produce
    /// an empty line entry.
    ///
    /// # Errors
    ///
    /// - [`IntermediateError::InvalidJson`] if any line is not valid JSON or
    ///   does not match the expected shape.
    /// - [`IntermediateError::UnsupportedSchemaVersion`] if a line's
    ///   `schema_version` exceeds [`SCHEMA_VERSION`].
    pub fn from_jsonl(input: &str) -> Result<Self, IntermediateError> {
        let mut lines = Vec::new();
        for raw in input.lines() {
            if raw.trim().is_empty() {
                continue;
            }
            let line: IntermediateLine = serde_json::from_str(raw)?;
            if line.schema_version > SCHEMA_VERSION {
                return Err(IntermediateError::UnsupportedSchemaVersion {
                    found: line.schema_version,
                    supported: SCHEMA_VERSION,
                });
            }
            lines.push(line);
        }
        Ok(Self { lines })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flag::Flag;
    use crate::placeholder::Placeholder;
    use crate::unit::{Provenance, Target, UnitState};

    fn sample_unit() -> Unit {
        let mut u = Unit::untranslated_singular("ctx::Hello {0}", "Hello {0}");
        u.placeholders.push(Placeholder::positional(0, 6));
        u.target = Target::Singular {
            text: Some("Hallo {0}".into()),
        };
        u.state = UnitState::Proposed;
        u.flags.insert(Flag::LowConfidence);
        u.provenance = Provenance {
            file: "src/main.cpp".into(),
            line: Some(42),
            byte_offset: None,
        };
        u
    }

    #[test]
    fn roundtrip_single_line() {
        let im = Intermediate::from_units([sample_unit()]);
        let s = im.to_jsonl().expect("serialize");
        assert!(s.ends_with('\n'));
        let back = Intermediate::from_jsonl(&s).expect("parse");
        assert_eq!(im, back);
    }

    #[test]
    fn roundtrip_multiple_lines_preserves_order() {
        let units: Vec<Unit> = (0..5)
            .map(|i| Unit::untranslated_singular(format!("id::{i}"), format!("src {i}")))
            .collect();
        let im = Intermediate::from_units(units.clone());
        let s = im.to_jsonl().unwrap();
        let back = Intermediate::from_jsonl(&s).unwrap();
        assert_eq!(back.lines.len(), 5);
        for (i, line) in back.lines.iter().enumerate() {
            assert_eq!(line.unit.id.as_str(), format!("id::{i}"));
        }
    }

    #[test]
    fn schema_version_is_emitted() {
        let im = Intermediate::from_units([sample_unit()]);
        let s = im.to_jsonl().unwrap();
        assert!(
            s.contains(&format!("\"schema_version\":{SCHEMA_VERSION}")),
            "expected schema_version field in {s}",
        );
    }

    #[test]
    fn future_schema_version_is_rejected() {
        let bad = format!(
            r#"{{"schema_version":{},"id":"x","source":"","target":{{"kind":"singular","text":null}},"placeholders":[],"plural_arity":null,"flags":[],"provenance":{{"file":"","line":null,"byte_offset":null}},"state":"untranslated"}}"#,
            SCHEMA_VERSION + 1,
        );
        let err = Intermediate::from_jsonl(&bad).unwrap_err();
        assert!(
            matches!(err, IntermediateError::UnsupportedSchemaVersion { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn blank_lines_are_skipped() {
        let s = "\n\n";
        let im = Intermediate::from_jsonl(s).unwrap();
        assert!(im.is_empty());
    }
}
