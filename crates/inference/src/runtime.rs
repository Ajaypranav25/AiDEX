use crate::{InferenceError, RuntimeInfo, RuntimeManifest, catalog, files};
use aidex_core::CancelToken;
use aidex_drive::DrivePaths;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};
pub(crate) const MANIFEST: &str = "runtime-manifest.json";
pub(crate) fn directory(paths: &DrivePaths) -> Result<PathBuf, InferenceError> {
    paths
        .platform_runtime()
        .map(|p| p.join("llama-cpp"))
        .ok_or_else(|| InferenceError::Unavailable("unsupported host platform".into()))
}
fn validate(manifest: &RuntimeManifest) -> Result<(), InferenceError> {
    if manifest.version != 1
        || manifest.platform != aidex_drive::platform_runtime_name().unwrap_or("")
        || manifest.files.is_empty()
        || manifest.files.len() > 256
    {
        return Err(InferenceError::Invalid(
            "runtime manifest version/platform/files mismatch".into(),
        ));
    }
    files::segment(&manifest.release)?;
    files::segment(&manifest.executable)?;
    if manifest.executable != format!("llama-server{}", std::env::consts::EXE_SUFFIX) {
        return Err(InferenceError::Invalid(
            "runtime executable must be llama-server".into(),
        ));
    }
    let mut seen = HashSet::new();
    for file in &manifest.files {
        files::segment(&file.path)?;
        if file.path == MANIFEST
            || !seen.insert(file.path.to_ascii_lowercase())
            || file.size_bytes == 0
            || file.sha256.len() != 64
            || !file.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(InferenceError::Integrity(
                "invalid runtime file declaration".into(),
            ));
        }
    }
    if !seen.contains(&manifest.executable.to_ascii_lowercase()) {
        return Err(InferenceError::Integrity(
            "runtime executable digest missing".into(),
        ));
    }
    Ok(())
}
fn verify(dir: &Path, m: &RuntimeManifest, cancel: &CancelToken) -> Result<(), InferenceError> {
    validate(m)?;
    files::safe_path(dir)?;
    let allowed = m
        .files
        .iter()
        .map(|f| f.path.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name != MANIFEST && !allowed.contains(&name.to_ascii_lowercase()) {
            return Err(InferenceError::Integrity(
                "runtime contains files outside checksum manifest".into(),
            ));
        }
    }
    for f in &m.files {
        let (hash, size) = files::digest(&dir.join(&f.path), cancel)?;
        if !hash.eq_ignore_ascii_case(&f.sha256) || size != f.size_bytes {
            return Err(InferenceError::Integrity(format!(
                "runtime checksum mismatch for {}",
                f.path
            )));
        }
    }
    Ok(())
}
pub(crate) fn inspect(
    paths: &DrivePaths,
) -> Result<Option<(RuntimeInfo, RuntimeManifest)>, InferenceError> {
    inspect_with_cancel(paths, &CancelToken::new())
}
pub(crate) fn inspect_with_cancel(
    paths: &DrivePaths,
    cancel: &CancelToken,
) -> Result<Option<(RuntimeInfo, RuntimeManifest)>, InferenceError> {
    files::check_cancel(cancel)?;
    let dir = directory(paths)?;
    if !dir.exists() {
        return Ok(None);
    }
    let manifest: RuntimeManifest = files::read_json(&dir.join(MANIFEST))?;
    validate(&manifest)?;
    let verified = match verify(&dir, &manifest, cancel) {
        Err(InferenceError::Cancelled) => return Err(InferenceError::Cancelled),
        result => result.is_ok(),
    };
    let info = RuntimeInfo {
        release: manifest.release.clone(),
        platform: manifest.platform.clone(),
        path: catalog::relative(paths, &dir)?,
        executable: manifest.executable.clone(),
        verified,
        gpu_supported: false,
    };
    Ok(Some((info, manifest)))
}
pub(crate) fn install(
    paths: &DrivePaths,
    source: Option<&Path>,
    cancel: &CancelToken,
) -> Result<RuntimeInfo, InferenceError> {
    files::check_cancel(cancel)?;
    let default;
    let source = match source {
        Some(s) => s,
        None => {
            default = std::env::var_os("AIDEX_LLAMA_DISTRIBUTION")
                .map(PathBuf::from)
                .unwrap_or_else(|| paths.runtime().join("components").join("llama-cpp"));
            &default
        }
    };
    let manifest: RuntimeManifest = files::read_json(&source.join(MANIFEST))?;
    verify(source, &manifest, cancel)?;
    let target = directory(paths)?;
    if target.exists() {
        return Err(InferenceError::Conflict(
            "runtime already installed; managed replacement is not supported".into(),
        ));
    }
    let parent = paths
        .ensure_dir(
            target
                .parent()
                .ok_or_else(|| InferenceError::Invalid("missing runtime parent".into()))?,
        )
        .map_err(|e| InferenceError::Io(e.to_string()))?;
    let needed = manifest.files.iter().try_fold(0u64, |sum, f| {
        sum.checked_add(f.size_bytes)
            .ok_or_else(|| InferenceError::Invalid("runtime size overflow".into()))
    })?;
    files::free_space(&parent, needed)?;
    let stage = files::Stage::new(&parent)?;
    for f in &manifest.files {
        let (hash, size) = files::copy(&source.join(&f.path), &stage.0.join(&f.path), cancel)?;
        if !hash.eq_ignore_ascii_case(&f.sha256) || size != f.size_bytes {
            return Err(InferenceError::Integrity(
                "runtime changed during install".into(),
            ));
        }
    }
    files::write_json(&stage.0.join(MANIFEST), &manifest)?;
    verify(&stage.0, &manifest, cancel)?;
    files::check_cancel(cancel)?;
    fs::rename(&stage.0, &target)?;
    inspect(paths)?
        .map(|p| p.0)
        .ok_or_else(|| InferenceError::Integrity("runtime disappeared after install".into()))
}
