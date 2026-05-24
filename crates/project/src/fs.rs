//! Filesystem abstraction for the project crate.
//!
//! All I/O this crate performs goes through [`ProjectFs`]. The production
//! implementation is [`RealFs`] (wraps `std::fs`); tests use [`InMemoryFs`]
//! (a `Mutex<BTreeMap>` — no `dashmap` or disk required).
//!
//! Atomicity contract for [`ProjectFs::write_atomic`]: the method must
//! guarantee that a reader never observes a partial write. `RealFs` achieves
//! this via write-to-temp + fsync + rename. `InMemoryFs` achieves it
//! trivially because the map swap under the mutex is atomic from any other
//! code running in the same process.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Filesystem abstraction for the project crate.
///
/// All methods take `&self` (shared reference) because both `RealFs` and
/// `InMemoryFs` use interior mutability. External impls (e.g. a future
/// S3-backed FS for cloud projects) should follow the same pattern.
///
/// # Path semantics
///
/// Callers always pass **absolute** paths. Implementations are not required
/// to resolve relative paths. `ProjectPaths` (slice b) provides resolved
/// absolute paths for every state file.
///
/// # Atomicity
///
/// `write_atomic` must guarantee readers never see a partial write (write to
/// temp, fsync, rename). `append` need only be POSIX-safe: writes of
/// `< PIPE_BUF` bytes (4 KiB on Linux/macOS) are atomic at the OS level.
pub trait ProjectFs: Send + Sync {
    /// Returns `true` if the path exists (file or directory).
    fn exists(&self, path: &Path) -> bool;

    /// Returns `true` if `path` is a regular file that exists.
    fn is_file(&self, path: &Path) -> bool;

    /// Returns `true` if `path` is a directory that exists.
    fn is_dir(&self, path: &Path) -> bool;

    /// Read the entire contents of `path` as raw bytes.
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;

    /// Read the entire contents of `path` as a UTF-8 string.
    fn read_to_string(&self, path: &Path) -> io::Result<String>;

    /// Atomically replace `path` with `contents`.
    ///
    /// Writes to a temporary file adjacent to `path`, fsyncs, then renames.
    /// A crash after the rename leaves `path` fully written; a crash before
    /// leaves the original `path` intact.
    fn write_atomic(&self, path: &Path, contents: &[u8]) -> io::Result<()>;

    /// Append `line` to `path`, creating the file if it does not exist.
    ///
    /// Used by the corrections store. Lines must be `< PIPE_BUF` (4 KiB) for
    /// the OS-level atomicity guarantee on POSIX systems.
    fn append(&self, path: &Path, line: &[u8]) -> io::Result<()>;

    /// Create `path` and all of its ancestors if they do not already exist.
    /// Idempotent.
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;

    /// List one level of `path` (non-recursive), sorted lexicographically.
    ///
    /// Returns each entry as its full (absolute) path. Does not recurse;
    /// callers that need recursive walks do so in crate code, not through
    /// this trait.
    fn list_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;
}

// ── RealFs ───────────────────────────────────────────────────────────────────

/// Production filesystem implementation backed by `std::fs`.
///
/// `write_atomic` uses the temp-file-then-rename idiom with `File::sync_all`
/// to guarantee durability before the rename.
pub struct RealFs;

impl ProjectFs for RealFs {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn write_atomic(&self, path: &Path, contents: &[u8]) -> io::Result<()> {
        // Place the temp file in the same directory so the rename is on the
        // same filesystem (cross-device rename is not atomic).
        let dir = path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "write_atomic: path has no parent directory",
            )
        })?;
        let tmp_path = dir.join(format!(
            ".{}.tmp.{}",
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown"),
            std::process::id()
        ));

        {
            let mut file = std::fs::File::create(&tmp_path)?;
            file.write_all(contents)?;
            file.sync_all()?;
        }

        std::fs::rename(&tmp_path, path).inspect_err(|_| {
            // Best-effort cleanup on rename failure; ignore cleanup errors.
            let _ = std::fs::remove_file(&tmp_path);
        })
    }

    fn append(&self, path: &Path, line: &[u8]) -> io::Result<()> {
        use std::fs::OpenOptions;
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        file.write_all(line)
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn list_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(path)?
            .map(|r| r.map(|e| e.path()))
            .collect::<io::Result<_>>()?;
        entries.sort();
        Ok(entries)
    }
}

// ── InMemoryFs ───────────────────────────────────────────────────────────────

/// In-memory filesystem for tests.
///
/// Backed by a `Mutex<BTreeMap<PathBuf, Vec<u8>>>`. Only constructable via
/// [`InMemoryFs::new`] — no public fields. The map models a flat namespace:
/// paths are the full keys; directories are implicit (they exist when any
/// entry under them exists, or when explicitly created via `create_dir_all`
/// which inserts a sentinel entry).
pub struct InMemoryFs {
    // Stores file contents keyed by absolute path. Directories are tracked as
    // zero-length entries so `is_dir` / `exists` work without scanning all keys.
    inner: Mutex<InMemoryState>,
}

struct InMemoryState {
    files: BTreeMap<PathBuf, Vec<u8>>,
    dirs: BTreeMap<PathBuf, ()>,
}

impl InMemoryFs {
    /// Create a new, empty in-memory filesystem.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(InMemoryState {
                files: BTreeMap::new(),
                dirs: BTreeMap::new(),
            }),
        }
    }
}

impl Default for InMemoryFs {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectFs for InMemoryFs {
    fn exists(&self, path: &Path) -> bool {
        let s = self.inner.lock().expect("InMemoryFs lock poisoned");
        s.files.contains_key(path) || s.dirs.contains_key(path)
    }

    fn is_file(&self, path: &Path) -> bool {
        let s = self.inner.lock().expect("InMemoryFs lock poisoned");
        s.files.contains_key(path)
    }

    fn is_dir(&self, path: &Path) -> bool {
        let s = self.inner.lock().expect("InMemoryFs lock poisoned");
        s.dirs.contains_key(path)
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let s = self.inner.lock().expect("InMemoryFs lock poisoned");
        s.files
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.display().to_string()))
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        let bytes = self.read(path)?;
        String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn write_atomic(&self, path: &Path, contents: &[u8]) -> io::Result<()> {
        // Trivially atomic under the mutex: the whole swap is a single map
        // insert visible to other threads only after the lock is released.
        let mut s = self.inner.lock().expect("InMemoryFs lock poisoned");
        // Ensure parent directories are visible.
        if let Some(parent) = path.parent() {
            insert_ancestor_dirs(&mut s.dirs, parent);
        }
        s.files.insert(path.to_path_buf(), contents.to_vec());
        Ok(())
    }

    fn append(&self, path: &Path, line: &[u8]) -> io::Result<()> {
        let mut s = self.inner.lock().expect("InMemoryFs lock poisoned");
        if let Some(parent) = path.parent() {
            insert_ancestor_dirs(&mut s.dirs, parent);
        }
        s.files
            .entry(path.to_path_buf())
            .or_default()
            .extend_from_slice(line);
        Ok(())
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        let mut s = self.inner.lock().expect("InMemoryFs lock poisoned");
        insert_ancestor_dirs(&mut s.dirs, path);
        Ok(())
    }

    fn list_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let s = self.inner.lock().expect("InMemoryFs lock poisoned");
        if !s.dirs.contains_key(path) && !s.files.contains_key(path) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                path.display().to_string(),
            ));
        }
        // Collect direct children: entries whose parent is exactly `path`.
        let mut results: Vec<PathBuf> = s
            .files
            .keys()
            .chain(s.dirs.keys())
            .filter(|p| p.parent() == Some(path))
            .cloned()
            .collect();
        results.sort();
        results.dedup();
        Ok(results)
    }
}

/// Insert `path` and all of its ancestors into the directory sentinel map.
fn insert_ancestor_dirs(dirs: &mut BTreeMap<PathBuf, ()>, path: &Path) {
    let mut current = path.to_path_buf();
    loop {
        dirs.insert(current.clone(), ());
        match current.parent() {
            Some(p) if p != current => current = p.to_path_buf(),
            _ => break,
        }
    }
}
