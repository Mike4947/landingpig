//! Filesystem manager — read, write, and context mapping.

pub mod context;
pub mod picker;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use self::context::{DependencyGraph, ingest_path};
pub use self::picker::{PickerEntryKind, list_picker_entries, picker_start_dir};

pub struct FileManager {
    workspace: PathBuf,
}

impl FileManager {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    #[allow(dead_code)]
    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    pub fn read(&self, path: &Path) -> Result<String> {
        let resolved = self.resolve(path)?;
        fs::read_to_string(&resolved)
            .with_context(|| format!("failed to read {}", resolved.display()))
    }

    pub fn write(&self, path: &Path, content: &str) -> Result<()> {
        let resolved = self.resolve(path)?;
        if let Some(parent) = resolved.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&resolved, content)
            .with_context(|| format!("failed to write {}", resolved.display()))
    }

    pub fn import(&self, path: &Path) -> Result<DependencyGraph> {
        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.workspace.join(path)
        };
        ingest_path(&resolved)
    }

    pub fn complete_paths(&self, prefix: &str) -> Vec<String> {
        let prefix = prefix.trim();

        let (search_dir, partial) = if prefix.is_empty() || prefix == "." {
            (self.workspace.clone(), String::new())
        } else {
            let search_root = if prefix.starts_with('/') {
                PathBuf::from(prefix)
            } else {
                self.workspace.join(prefix)
            };

            let parent = search_root
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| self.workspace.clone());
            let partial = search_root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(prefix)
                .to_string();
            (parent, partial)
        };

        if !search_dir.exists() {
            return Vec::new();
        }

        let mut results = Vec::new();
        if let Ok(entries) = fs::read_dir(&search_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if partial.is_empty() || name.starts_with(&partial) {
                    let full = entry.path();
                    let rel = full
                        .strip_prefix(&self.workspace)
                        .unwrap_or(&full)
                        .display()
                        .to_string();
                    results.push(rel);
                }
            }
        }

        results.sort();
        results.truncate(20);
        results
    }

    fn resolve(&self, path: &Path) -> Result<PathBuf> {
        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.workspace.join(path)
        };

        let canonical_workspace = self
            .workspace
            .canonicalize()
            .unwrap_or_else(|_| self.workspace.clone());
        let canonical_resolved = resolved
            .canonicalize()
            .unwrap_or(resolved.clone());

        if !canonical_resolved.starts_with(&canonical_workspace) {
            anyhow::bail!("path escapes workspace: {}", path.display());
        }

        Ok(canonical_resolved)
    }
}
