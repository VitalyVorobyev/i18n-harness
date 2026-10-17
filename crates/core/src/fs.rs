//! Filesystem helpers shared by every crate that writes catalogs or state.

use std::io::{self, Write};
use std::path::Path;

/// Write `contents` to `path` atomically.
///
/// The bytes go to a temporary file in the same directory (a cross-device
/// rename is not atomic), are flushed to disk, and the temporary file is then
/// renamed over `path`. A failure at any point leaves the previous `path`
/// untouched, so a crash mid-save never leaves a half-written catalog.
///
/// # Errors
///
/// Returns the underlying I/O error. `path` without a parent directory is an
/// [`io::ErrorKind::InvalidInput`] error.
pub fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let dir = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "write_atomic: path has no parent directory",
        )
    })?;
    let dir = if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    };
    let tmp_path = dir.join(format!(
        ".{}.tmp-i18n-harness.{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("file"),
        std::process::id()
    ));

    let written = (|| {
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(contents)?;
        file.sync_all()
    })();
    if let Err(err) = written {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(err);
    }

    std::fs::rename(&tmp_path, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp_path);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_existing_file_and_leaves_no_temp_file() {
        let dir = std::env::temp_dir().join(format!("i18n-harness-core-fs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("catalog.ts");
        std::fs::write(&path, b"old").unwrap();

        write_atomic(&path, b"new").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains("tmp-i18n-harness"))
            .collect();
        assert!(leftovers.is_empty(), "temp file left behind: {leftovers:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_directory_is_an_error_and_writes_nothing() {
        let path = std::env::temp_dir()
            .join(format!("i18n-harness-missing-{}", std::process::id()))
            .join("x.po");
        assert!(write_atomic(&path, b"data").is_err());
        assert!(!path.exists());
    }
}
