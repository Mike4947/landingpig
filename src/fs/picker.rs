//! Directory listing for the workspace import picker.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerEntryKind {
    ImportHere,
    Parent,
    Drive,
    Directory,
}

#[derive(Debug, Clone)]
pub struct PickerEntry {
    pub label: String,
    pub path: PathBuf,
    pub kind: PickerEntryKind,
}

const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".next",
    "dist",
    "build",
    ".cache",
];

const MOUNT_ROOTS: &[&str] = &["/media", "/run/media", "/mnt"];

pub fn discover_drives() -> Vec<PathBuf> {
    let mut drives = Vec::new();
    let mut seen = HashSet::new();

    for root in MOUNT_ROOTS {
        let root_path = PathBuf::from(root);
        if root_path.is_dir() {
            scan_mount_root(&root_path, &root_path, &mut drives, &mut seen);
        }
    }

    drives.sort_by(|a, b| {
        drive_label(a)
            .to_lowercase()
            .cmp(&drive_label(b).to_lowercase())
    });
    drives
}

fn scan_mount_root(
    root: &Path,
    current: &Path,
    drives: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
) {
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }

        if is_drive_mount(root, &path) {
            add_drive(&path, drives, seen);
        } else {
            scan_mount_root(root, &path, drives, seen);
        }
    }
}

fn is_drive_mount(root: &Path, path: &Path) -> bool {
    let root_name = root.to_string_lossy();
    let depth = path
        .strip_prefix(root)
        .map(|rel| rel.components().count())
        .unwrap_or(0);

    match root_name.as_ref() {
        "/mnt" => depth == 1,
        "/media" | "/run/media" => depth == 2,
        _ => false,
    }
}

fn add_drive(path: &Path, drives: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>) {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if seen.insert(canonical.clone()) {
        drives.push(canonical);
    }
}

fn drive_label(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| path.display().to_string())
}

fn drive_entry_label(path: &Path, all_drives: &[PathBuf]) -> String {
    let name = drive_label(path);
    let duplicates = all_drives
        .iter()
        .filter(|d| drive_label(d) == name)
        .count();
    if duplicates > 1 {
        format!("{name}  ({})", path.display())
    } else {
        name
    }
}

fn should_show_drive_jump(current: &Path, drive: &Path) -> bool {
    if current == drive {
        return false;
    }
    // Already inside this drive — no need for a shortcut.
    current.starts_with(drive)
}

pub fn list_picker_entries(dir: &Path) -> Result<Vec<PickerEntry>> {
    let canonical = dir
        .canonicalize()
        .with_context(|| format!("cannot access {}", dir.display()))?;

    let mut entries = vec![PickerEntry {
        label: "Import this folder".to_string(),
        path: canonical.clone(),
        kind: PickerEntryKind::ImportHere,
    }];

    if let Some(parent) = canonical.parent() {
        entries.push(PickerEntry {
            label: "..".to_string(),
            path: parent.to_path_buf(),
            kind: PickerEntryKind::Parent,
        });
    }

    let drives = discover_drives();
    for drive in &drives {
        if should_show_drive_jump(&canonical, drive) {
            entries.push(PickerEntry {
                label: drive_entry_label(drive, &drives),
                path: drive.clone(),
                kind: PickerEntryKind::Drive,
            });
        }
    }

    let mut dirs = Vec::new();
    for entry in fs::read_dir(&canonical)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if SKIP_DIRS.contains(&name.as_str()) {
            continue;
        }
        dirs.push(PickerEntry {
            label: name,
            path,
            kind: PickerEntryKind::Directory,
        });
    }

    dirs.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    entries.extend(dirs);
    Ok(entries)
}

pub fn picker_start_dir(fallback: &Path) -> PathBuf {
    if let Ok(canon) = fallback.canonicalize() {
        if canon.is_dir() {
            return canon;
        }
    }

    dirs::home_dir()
        .unwrap_or_else(|| fallback.to_path_buf())
        .canonicalize()
        .unwrap_or_else(|_| fallback.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_run_media_drives() {
        let drives = discover_drives();
        let joined: Vec<String> = drives.iter().map(|d| d.display().to_string()).collect();
        // Typical Ubuntu removable drive path
        if Path::new("/run/media").is_dir() {
            assert!(
                joined.iter().any(|p| p.contains("/run/media/") || p.contains("/media/")),
                "expected mounted drive paths, got: {joined:?}"
            );
        }
    }
}
