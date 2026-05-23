//! Error types for the Qt `.ts` adapter.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Errors that can occur while extracting a `.ts` file into a [`crate::Catalog`].
#[derive(Debug, Error)]
pub enum ExtractError {
    /// The file could not be read from disk.
    ///
    /// Occurs on permission errors, missing files, or other I/O problems.
    /// Caller should surface the path; the harness has no fallback.
    #[error("failed to read {path}: {source}")]
    Io {
        /// Path the harness tried to read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// The XML parser failed to parse the document.
    ///
    /// Occurs on malformed XML — unclosed tags, bad encoding declaration,
    /// invalid entities. The Qt Linguist file is presumed valid; if this
    /// fires the file was corrupted by some other tool and the harness must
    /// not guess at what was intended.
    #[error("invalid Qt .ts XML in {path}: {source}")]
    Xml {
        /// Path the harness was reading.
        path: PathBuf,
        /// Underlying `quick-xml` error.
        #[source]
        source: quick_xml::Error,
    },

    /// The document is well-formed XML but is not a Qt `.ts` file
    /// (no `<TS>` root element).
    ///
    /// Occurs when a user points the harness at a non-Qt XML file by
    /// mistake. Caller should report the path and the expected format.
    #[error("not a Qt .ts file (missing <TS> root): {path}")]
    NotTsFile {
        /// Path the harness tried to read.
        path: PathBuf,
    },
}

/// Errors that can occur while applying units back to a `.ts` file.
#[derive(Debug, Error)]
pub enum ApplyError {
    /// The output file could not be written.
    ///
    /// Occurs on permission errors or full disks. The catalog state is
    /// unchanged on disk (we write to a temp path and rename only on
    /// success).
    #[error("failed to write {path}: {source}")]
    Io {
        /// Output path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// A unit passed to `apply` does not correspond to any unit in the
    /// catalog (its [`crate::Unit::id`] is not present).
    ///
    /// Occurs when the caller hands us a unit from a different file. We
    /// refuse rather than silently dropping it. Caller should ensure the
    /// `units` slice was originally derived from this catalog's `extract`.
    #[error("unit id {id} is not present in this catalog")]
    UnknownUnit {
        /// The offending unit id.
        id: String,
    },
}
