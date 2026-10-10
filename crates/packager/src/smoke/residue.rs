//! Host-residue check: list well-known per-user directories before and after the
//! smoke run and report entries that appeared.
//!
//! This is a top-level listing, not a full filesystem audit: it catches new files or
//! directories directly inside the home directory and the per-user config, data,
//! cache, and state roots, plus AiDEX-named entries in the system temp directory
//! (other processes create unrelated temp files concurrently). Writes deeper inside
//! pre-existing directories and changes to existing entries are not detected.
//! macOS Preferences also excludes the exact regular file
//! `com.apple.siri.ODDI.ExperimentationWorker.plist`, maintained by the OS in the
//! background; other entries there remain fully watched.

use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    ffi::{OsStr, OsString},
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
    /// All new entries except one known OS-maintained regular plist.
    MacosPreferences,
}

/// Entry names mapped to their regular-file status at snapshot time.
type Entries = BTreeMap<OsString, bool>;

#[derive(Debug)]
pub struct Snapshot {
    dirs: Vec<(PathBuf, Watch, Option<Entries>)>,
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
                let watch = if sub == "Preferences" {
                    Watch::MacosPreferences
                } else {
                    Watch::All
                };
                dirs.push((home.join("Library").join(sub), watch));
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
                if *watch != Watch::AidexNamed {
                    found.insert(dir.clone());
                }
                continue;
            };
            for (name, regular_file) in now {
                if before.contains_key(name) {
                    continue;
                }
                // Siri's background worker can create this plist during smoke.
                // Restrict the exception to this directory policy, exact name,
                // and regular-file type captured without following symlinks.
                let os_preference = *watch == Watch::MacosPreferences
                    && *regular_file
                    && name == OsStr::new("com.apple.siri.ODDI.ExperimentationWorker.plist");
                let named = name
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .contains("aidex");
                if (*watch != Watch::AidexNamed || named) && !os_preference {
                    found.insert(dir.join(name));
                }
            }
        }
        found.into_iter().collect()
    }
}

/// Entry names and whether each was a regular file at snapshot time. An unknown
/// file type is never eligible for the OS-owned regular-file exception.
fn list(dir: &Path) -> Option<Entries> {
    let entries = fs::read_dir(dir).ok()?;
    Some(
        entries
            .flatten()
            .map(|entry| {
                (
                    entry.file_name(),
                    entry.file_type().is_ok_and(|kind| kind.is_file()),
                )
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_preferences_background_churn_does_not_hide_host_writes() {
        let host = tempfile::tempdir().unwrap();
        let preferences = host.path().join("Library/Preferences");
        fs::create_dir_all(&preferences).unwrap();
        let dirs = vec![(preferences.clone(), Watch::MacosPreferences)];
        let before = Snapshot::take(&dirs);

        // A separate background writer reproduces the runner's Siri preference
        // appearing between snapshots; joining makes the fixture deterministic.
        let background_preferences = preferences.clone();
        let background = std::thread::spawn(move || {
            fs::write(
                background_preferences.join("com.apple.siri.ODDI.ExperimentationWorker.plist"),
                "OS-owned preference",
            )
            .unwrap();
        });
        for name in [
            "unexpected.plist",
            "com.apple.unexpected.plist",
            "AiDEX.plist",
            "com.apple.siri.ODDI.ExperimentationWorker.plist.bak",
            "com.apple.siri.ODDI.ExperimentationWorker-AiDEX.plist",
        ] {
            fs::write(preferences.join(name), "unexpected host write").unwrap();
        }
        fs::create_dir(preferences.join("AiDEX-data")).unwrap();
        background.join().unwrap();

        let mut expected = vec![
            preferences.join("unexpected.plist"),
            preferences.join("com.apple.unexpected.plist"),
            preferences.join("AiDEX.plist"),
            preferences.join("com.apple.siri.ODDI.ExperimentationWorker.plist.bak"),
            preferences.join("com.apple.siri.ODDI.ExperimentationWorker-AiDEX.plist"),
            preferences.join("AiDEX-data"),
        ];
        expected.sort();
        assert_eq!(before.new_entries(&Snapshot::take(&dirs)), expected);
    }

    #[test]
    fn differently_cased_macos_preference_name_is_audited() {
        let preferences = tempfile::tempdir().unwrap();
        let dirs = vec![(preferences.path().to_path_buf(), Watch::MacosPreferences)];
        let before = Snapshot::take(&dirs);
        // Separate from the OS-owned filename fixture for case-insensitive hosts.
        let unexpected = preferences
            .path()
            .join("Com.apple.siri.ODDI.ExperimentationWorker.plist");
        fs::write(&unexpected, "unexpected host write").unwrap();
        assert_eq!(before.new_entries(&Snapshot::take(&dirs)), vec![unexpected]);
    }

    #[test]
    fn known_macos_preference_name_is_audited_in_other_directories() {
        let host = tempfile::tempdir().unwrap();
        let cache = host.path().join("Library/Caches");
        fs::create_dir_all(&cache).unwrap();
        let dirs = vec![
            (host.path().to_path_buf(), Watch::All),
            (cache.clone(), Watch::All),
        ];
        let before = Snapshot::take(&dirs);
        let name = "com.apple.siri.ODDI.ExperimentationWorker.plist";
        fs::write(host.path().join(name), "unexpected host write").unwrap();
        fs::write(cache.join(name), "unexpected cache write").unwrap();
        let mut expected = vec![host.path().join(name), cache.join(name)];
        expected.sort();
        assert_eq!(before.new_entries(&Snapshot::take(&dirs)), expected);
    }

    #[test]
    fn known_macos_preference_name_does_not_exclude_a_directory() {
        let preferences = tempfile::tempdir().unwrap();
        let dirs = vec![(preferences.path().to_path_buf(), Watch::MacosPreferences)];
        let before = Snapshot::take(&dirs);
        let unexpected = preferences
            .path()
            .join("com.apple.siri.ODDI.ExperimentationWorker.plist");
        fs::create_dir(&unexpected).unwrap();
        assert_eq!(before.new_entries(&Snapshot::take(&dirs)), vec![unexpected]);
    }

    #[test]
    fn macos_preference_file_type_is_captured_in_the_snapshot() {
        let preferences = tempfile::tempdir().unwrap();
        let dirs = vec![(preferences.path().to_path_buf(), Watch::MacosPreferences)];
        let before = Snapshot::take(&dirs);
        let unexpected = preferences
            .path()
            .join("com.apple.siri.ODDI.ExperimentationWorker.plist");
        fs::create_dir(&unexpected).unwrap();
        let after = Snapshot::take(&dirs);
        fs::remove_dir(&unexpected).unwrap();
        fs::write(
            &unexpected,
            "regular file replacing an unexpected directory",
        )
        .unwrap();
        assert_eq!(before.new_entries(&after), vec![unexpected]);
    }

    #[test]
    fn newly_created_macos_preferences_directory_is_audited() {
        let host = tempfile::tempdir().unwrap();
        let preferences = host.path().join("Library/Preferences");
        let dirs = vec![(preferences.clone(), Watch::MacosPreferences)];
        let before = Snapshot::take(&dirs);
        fs::create_dir_all(&preferences).unwrap();
        fs::write(
            preferences.join("com.apple.siri.ODDI.ExperimentationWorker.plist"),
            "OS-owned preference",
        )
        .unwrap();
        assert_eq!(
            before.new_entries(&Snapshot::take(&dirs)),
            vec![preferences]
        );
    }

    #[cfg(unix)]
    #[test]
    fn known_macos_preference_name_does_not_exclude_a_symlink() {
        let preferences = tempfile::tempdir().unwrap();
        let target = preferences.path().join("existing.plist");
        fs::write(&target, "existing preference").unwrap();
        let dirs = vec![(preferences.path().to_path_buf(), Watch::MacosPreferences)];
        let before = Snapshot::take(&dirs);
        let unexpected = preferences
            .path()
            .join("com.apple.siri.ODDI.ExperimentationWorker.plist");
        std::os::unix::fs::symlink(target, &unexpected).unwrap();
        assert_eq!(before.new_entries(&Snapshot::take(&dirs)), vec![unexpected]);
    }

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
