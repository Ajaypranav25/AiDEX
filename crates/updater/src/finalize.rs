//! Steps after the pointer switch: launchers, provenance copies, and pruning. The
//! switch is the commit point, so these never undo it; a failure is a warning, and
//! recovery repeats them (they are idempotent) if the journal was not finalized.

use std::{collections::BTreeSet, fs, path::Path};

use aidex_bundle::{
    BundleManifest, CurrentPointer, LauncherPlacement, MANIFEST_FILE_NAME, SIGNATURE_FILE_NAME,
    layout::versions_dir, read_manifest, steps, write_atomic,
};

use crate::{
    Updater,
    sbom::{SBOM_BUNDLE_PATH, SBOM_FILE_NAME},
    service::running_bundle_dir,
};

/// Version directories kept per target: the active bundle and its rollback candidate.
pub const KEEP_VERSIONS: usize = 2;

impl Updater {
    /// Place the drive-root launchers from the newly active bundle.
    pub(crate) fn place_launchers(
        &self,
        bundle_dir: &Path,
        manifest: &BundleManifest,
        warnings: &mut Vec<String>,
    ) -> Vec<LauncherPlacement> {
        match steps::place_launchers(&self.paths, bundle_dir, manifest) {
            Ok(placements) => placements,
            Err(error) => {
                warnings.push(format!(
                    "the drive-root launcher was not updated ({error}); the previous launcher still verifies and runs the new bundle"
                ));
                Vec::new()
            }
        }
    }

    /// Copy `bundle.json`, `bundle.sig`, and the SBOM of an installed bundle to
    /// `manifests/<version dir name>/` (provenance that outlives pruning).
    pub(crate) fn copy_provenance(
        &self,
        bundle_dir: &Path,
        name: &str,
        warnings: &mut Vec<String>,
    ) {
        let result = (|| -> Result<(), String> {
            let root = self
                .paths
                .ensure_dir(&self.paths.manifests())
                .map_err(|error| error.to_string())?;
            let destination = self
                .paths
                .ensure_dir(&root.join(name))
                .map_err(|error| error.to_string())?;
            let sbom_source = SBOM_BUNDLE_PATH
                .split('/')
                .fold(bundle_dir.to_path_buf(), |path, part| path.join(part));
            for (source, file) in [
                (bundle_dir.join(MANIFEST_FILE_NAME), MANIFEST_FILE_NAME),
                (bundle_dir.join(SIGNATURE_FILE_NAME), SIGNATURE_FILE_NAME),
                (sbom_source, SBOM_FILE_NAME),
            ] {
                let metadata = match fs::symlink_metadata(&source) {
                    Ok(metadata) => metadata,
                    Err(_) => continue,
                };
                if !metadata.is_file() || metadata.len() > 64 * 1024 * 1024 {
                    continue;
                }
                let bytes = fs::read(&source).map_err(|error| error.to_string())?;
                write_atomic(&destination.join(file), &bytes).map_err(|error| error.to_string())?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            warnings.push(format!(
                "provenance was not copied to manifests/{name}/: {error}"
            ));
        }
    }

    /// Delete this target's version directories that neither the pointer (any
    /// target's current or previous entry) nor the running executable uses. Returns
    /// the removed names.
    pub(crate) fn prune(
        &self,
        pointer: &CurrentPointer,
        tag: &str,
        warnings: &mut Vec<String>,
    ) -> Vec<String> {
        let mut keep: BTreeSet<String> = BTreeSet::new();
        for entry in pointer.targets.values() {
            keep.insert(entry.dir.clone());
            if let Some(previous) = &entry.previous {
                keep.insert(previous.dir.clone());
            }
        }
        if let Some(running) = running_bundle_dir(&self.paths) {
            keep.insert(running);
        }
        let versions = versions_dir(&self.paths);
        let Ok(entries) = fs::read_dir(&versions) else {
            return Vec::new();
        };
        let mut candidates: Vec<String> = entries
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
            .filter(|name| !keep.contains(&aidex_bundle::pointer_dir(name)))
            .collect();
        candidates.sort();
        let marker = format!("-{}", self.target);
        let mut pruned = Vec::new();
        for name in candidates {
            let ours = match read_manifest(&versions.join(&name)) {
                Ok((manifest, _)) => manifest.target == self.target,
                // A damaged copy of this target's bundle (its name says so).
                Err(_) => name.contains(&marker),
            };
            if !ours {
                continue;
            }
            match self.remove_version_dir(&name, tag) {
                Ok(()) => pruned.push(name),
                Err(error) => warnings.push(format!("could not prune `versions/{name}`: {error}")),
            }
        }
        pruned
    }
}
