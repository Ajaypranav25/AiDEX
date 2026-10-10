//! Host-residue check: list well-known per-user directories before and after the
//! smoke run and report entries that appeared.
//!
//! This is a top-level listing, not a full filesystem audit: it catches new files or
//! directories directly inside the home directory and the per-user config, data,
//! cache, and state roots, plus AiDEX-named entries in the system temp directory
//! (other processes create unrelated temp files concurrently). Writes deeper inside
//! pre-existing directories are not detected.

use std::{
    collections::BTreeSet,
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Watch {
    /// Any new entry is residue.
    All,
    /// Only new entries whose name contains `aidex` (case-insensitive).
    AidexNamed,
}

#[derive(Debug)]
pub struct Snapshot {
    dirs: Vec<(PathBuf, Watch, Option<BTreeSet<OsString>>)>,
}

/// Directories to watch on this host.
pub fn watched() -> Vec<(PathBuf, Watch)> {
    let var = |name: &str| {
        env::var_os(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    let home = if cfg!(windows) {
        var("USERPROFILE")
    } else {
        var("HOME")
    };
    let mut dirs = Vec::new();
    if let Some(home) = &home {
        dirs.push((home.clone(), Watch::All));
    }
    if cfg!(windows) {
        for name in ["APPDATA", "LOCALAPPDATA"] {
            if let Some(dir) = var(name) {
                dirs.push((dir, Watch::All));
            }
        }
    } else if let Some(home) = &home {
        let xdg = |name: &str, default: &str| var(name).unwrap_or_else(|| home.join(default));
        if cfg!(target_os = "macos") {
            for sub in ["Application Support", "Caches", "Preferences", "Logs"] {
                dirs.push((home.join("Library").join(sub), Watch::All));
            }
        } else {
            dirs.push((xdg("XDG_CONFIG_HOME", ".config"), Watch::All));
            dirs.push((xdg("XDG_DATA_HOME", ".local/share"), Watch::All));
            dirs.push((xdg("XDG_STATE_HOME", ".local/state"), Watch::All));
            dirs.push((xdg("XDG_CACHE_HOME", ".cache"), Watch::All));
        }
    }
    dirs.push((env::temp_dir(), Watch::AidexNamed));
    let mut seen = BTreeSet::new();
    dirs.retain(|(dir, _)| seen.insert(dir.clone()));
    dirs
}

impl Snapshot {
    pub fn take(dirs: &[(PathBuf, Watch)]) -> Self {
        Self {
            dirs: dirs
                .iter()
                .map(|(dir, watch)| (dir.clone(), *watch, list(dir)))
                .collect(),
        }
    }

    pub fn dirs(&self) -> Vec<PathBuf> {
        self.dirs.iter().map(|(dir, _, _)| dir.clone()).collect()
    }

    /// Entries present in `after` but not in `self`, filtered per directory.
    pub fn new_entries(&self, after: &Self) -> Vec<PathBuf> {
        let mut found = BTreeSet::new();
        for ((dir, watch, before), (_, _, now)) in self.dirs.iter().zip(&after.dirs) {
            let Some(now) = now else { continue };
            let Some(before) = before else {
                // The directory itself appeared.
                if *watch == Watch::All {
                    found.insert(dir.clone());
                }
                continue;
            };
            for name in now.difference(before) {
                let named = name
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .contains("aidex");
                if *watch == Watch::All || named {
                    found.insert(dir.join(name));
                }
            }
        }
        found.into_iter().collect()
    }
}

fn list(dir: &Path) -> Option<BTreeSet<OsString>> {
    let entries = fs::read_dir(dir).ok()?;
    Some(entries.flatten().map(|entry| entry.file_name()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_entries_respect_the_watch_mode() {
        let all = tempfile::tempdir().unwrap();
        let named = tempfile::tempdir().unwrap();
        fs::write(all.path().join("existing"), "").unwrap();
        let dirs = vec![
            (all.path().to_path_buf(), Watch::All),
            (named.path().to_path_buf(), Watch::AidexNamed),
            (all.path().join("later"), Watch::All),
        ];
        let before = Snapshot::take(&dirs);
        fs::write(all.path().join("new-file"), "").unwrap();
        fs::write(named.path().join("unrelated.tmp"), "").unwrap();
        fs::write(named.path().join("AiDEX-residue"), "").unwrap();
        fs::create_dir(all.path().join("later")).unwrap();
        let after = Snapshot::take(&dirs);
        let found = before.new_entries(&after);
        let mut expected = vec![
            all.path().join("new-file"),
            all.path().join("later"),
            named.path().join("AiDEX-residue"),
        ];
        expected.sort();
        assert_eq!(found, expected);
        assert!(Snapshot::take(&dirs).new_entries(&after).is_empty());
    }
}
