//! The overlay-then-disk [`FileLoader`] that drives the resilient project walk.

use std::path::{Path, PathBuf};

use inference_project_model::{read_source_file, FileLoader};
use inference_vfs::Vfs;

/// A [`FileLoader`] that consults the editor's in-memory overlay first and falls
/// back to disk.
///
/// This is the reader ide-db hands to `inference_project_model::load_project_resilient`, so an
/// open, unsaved buffer shadows its on-disk contents while imports the editor
/// has never opened are still read from disk. It is the IDE half of the single
/// import-resolution seam the compiler and the IDE share (the compiler passes a
/// `DiskLoader`); the two can therefore never disagree about which files a
/// program imports.
///
/// # Case-insensitive filesystems
///
/// On macOS and Windows the import walk can derive a path whose casing differs
/// from the URI the editor interned an open buffer under (source says
/// `use lib::Math;` but the file is `lib/math.inf`). A case-sensitive `Path`
/// compare misses the overlay, yet the disk read succeeds on the case-insensitive
/// volume and would serve stale text that no later `didChange` invalidates. On an
/// overlay miss the loader therefore retries the overlay under the file's on-disk
/// canonical spelling (see [`overlay_text`]) before falling back to disk.
pub(crate) struct VfsLoader<'a> {
    vfs: &'a Vfs,
}

impl<'a> VfsLoader<'a> {
    /// Reads through `vfs`'s overlay, falling back to disk for files the overlay
    /// does not hold.
    #[must_use = "a loader does nothing until it is handed to the closure walk"]
    pub(crate) fn new(vfs: &'a Vfs) -> Self {
        Self { vfs }
    }
}

impl FileLoader for VfsLoader<'_> {
    fn exists(&self, path: &Path) -> bool {
        // An open buffer counts as existing even before it is written to disk;
        // otherwise fall back to the real filesystem.
        self.vfs.contents_of_path(path).is_some() || path.is_file()
    }

    fn read(&self, path: &Path) -> std::io::Result<String> {
        if let Some(text) = overlay_text(self.vfs, path, |candidate| {
            std::fs::canonicalize(candidate).ok()
        }) {
            return Ok(text);
        }
        read_source_file(path)
    }
}

/// The overlay text that should serve a read of `path`, or `None` when the caller
/// should fall back to disk.
///
/// The overlay is consulted directly first. On a miss, `path` is resolved through
/// `canonicalize` — which yields the file's on-disk spelling (its true casing on
/// a case-insensitive filesystem, matching the URI the editor interned an open
/// buffer under) and, in the production reader, only succeeds for a file that
/// exists — and the overlay is retried under that spelling. `canonicalize` is a
/// parameter so this retry is unit-testable without a real case-insensitive
/// filesystem.
fn overlay_text(
    vfs: &Vfs,
    path: &Path,
    canonicalize: impl FnOnce(&Path) -> Option<PathBuf>,
) -> Option<String> {
    if let Some(text) = vfs.contents_of_path(path) {
        return Some(text.to_string());
    }
    let canonical = canonicalize(path)?;
    // The direct lookup already covered the exact spelling; only a differing
    // canonical spelling can hold an overlay this compare missed.
    if canonical.as_path() == path {
        return None;
    }
    vfs.contents_of_path(&canonical)
        .map(|text| text.to_string())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use inference_project_model::FileLoader;
    use inference_vfs::Vfs;

    use super::{overlay_text, VfsLoader};

    #[test]
    fn overlay_hit_returns_buffer_text() {
        let mut vfs = Vfs::default();
        let id = vfs.intern(Path::new("/root/lib/math.inf"));
        vfs.set_contents(id, Arc::from("open buffer"));
        // An exact overlay hit never consults the canonicalizer.
        let text = overlay_text(&vfs, Path::new("/root/lib/math.inf"), |_| {
            panic!("canonicalize must not run on an exact overlay hit")
        });
        assert_eq!(text.as_deref(), Some("open buffer"));
    }

    #[test]
    fn miscased_path_resolves_to_the_overlay_via_canonicalization() {
        // The buffer is interned under the on-disk spelling `lib/math.inf`; the
        // walk derives `lib/Math.inf`. The case-sensitive overlay compare misses,
        // but canonicalization maps the derived path to the on-disk spelling, so
        // the retry serves the open buffer rather than stale disk text.
        let mut vfs = Vfs::default();
        let id = vfs.intern(Path::new("/root/lib/math.inf"));
        vfs.set_contents(id, Arc::from("live edits"));
        let text = overlay_text(&vfs, Path::new("/root/lib/Math.inf"), |_| {
            Some(PathBuf::from("/root/lib/math.inf"))
        });
        assert_eq!(text.as_deref(), Some("live edits"));
    }

    #[test]
    fn canonical_path_without_an_overlay_falls_through_to_disk() {
        // The path canonicalizes to a different spelling, but no overlay exists
        // under it, so the caller is told to read from disk (`None`).
        let vfs = Vfs::default();
        let text = overlay_text(&vfs, Path::new("/root/lib/Math.inf"), |_| {
            Some(PathBuf::from("/root/lib/math.inf"))
        });
        assert_eq!(text, None);
    }

    #[test]
    fn non_existent_path_does_not_borrow_another_buffer() {
        // A path that cannot be canonicalized (the production reader's "does not
        // exist" signal) must not serve any overlay — the read falls to disk.
        let mut vfs = Vfs::default();
        let id = vfs.intern(Path::new("/root/lib/math.inf"));
        vfs.set_contents(id, Arc::from("live edits"));
        let text = overlay_text(&vfs, Path::new("/root/lib/Math.inf"), |_| None);
        assert_eq!(text, None);
    }

    #[test]
    fn self_canonicalizing_path_does_not_retry() {
        // When canonicalization returns the same spelling (no case difference),
        // the already-missed direct lookup is not repeated and disk wins.
        let mut vfs = Vfs::default();
        let id = vfs.intern(Path::new("/root/lib/math.inf"));
        vfs.set_contents(id, Arc::from("live edits"));
        let text = overlay_text(&vfs, Path::new("/root/lib/Math.inf"), |p| {
            Some(p.to_path_buf())
        });
        assert_eq!(text, None);
    }

    #[test]
    fn disk_fallback_strips_a_utf8_bom() {
        // An unopened closure file read straight from disk goes through the shared
        // BOM-stripping reader, so its bytes match the buffer a client would send
        // for the same file (clients drop the BOM on open).
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir()
            .join(format!("inference-loader-bom-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("mod.inf");
        std::fs::write(&file, "\u{feff}pub fn f() -> i32 { return 0; }").unwrap();

        let vfs = Vfs::default();
        let loader = VfsLoader::new(&vfs);
        let text = loader.read(&file).expect("disk read");
        assert_eq!(text, "pub fn f() -> i32 { return 0; }");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
