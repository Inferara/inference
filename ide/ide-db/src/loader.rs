//! The overlay-then-disk [`FileLoader`] that drives the resilient project walk.

use std::path::Path;

use inference::FileLoader;
use inference_vfs::Vfs;

/// A [`FileLoader`] that consults the editor's in-memory overlay first and falls
/// back to disk.
///
/// This is the reader ide-db hands to `inference::load_project_resilient`, so an
/// open, unsaved buffer shadows its on-disk contents while imports the editor
/// has never opened are still read from disk. It is the IDE half of the single
/// import-resolution seam the compiler and the IDE share (the compiler passes a
/// `DiskLoader`); the two can therefore never disagree about which files a
/// program imports.
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
        match self.vfs.contents_of_path(path) {
            Some(text) => Ok(text.to_string()),
            None => std::fs::read_to_string(path),
        }
    }
}
