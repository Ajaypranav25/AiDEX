use crate::{Availability, InferenceError, ManagedModel, files, gguf};
use aidex_core::CancelToken;
use aidex_drive::DrivePaths;
use std::{fs, path::Path};
pub(crate) fn validate_id(id: &str) -> Result<(), InferenceError> {
    if id.len() != 66
        || !id.starts_with("m-")
        || !id[2..]
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(InferenceError::Invalid("invalid managed model id".into()));
    }
    Ok(())
}
pub(crate) fn relative(paths: &DrivePaths, path: &Path) -> Result<String, InferenceError> {
    Ok(path
        .strip_prefix(paths.root())
        .map_err(|_| InferenceError::Invalid("path outside selected drive".into()))?
        .to_string_lossy()
        .replace('\\', "/"))
}
pub(crate) fn list(paths: &DrivePaths) -> Result<Vec<ManagedModel>, InferenceError> {
    list_with_cancel(paths, &CancelToken::new())
}
pub(crate) fn list_with_cancel(
    paths: &DrivePaths,
    cancel: &CancelToken,
) -> Result<Vec<ManagedModel>, InferenceError> {
    let mut models = Vec::new();
    let dir = paths
        .ensure_dir(&paths.models())
        .map_err(|e| InferenceError::Io(e.to_string()))?;
    for entry in fs::read_dir(dir)? {
        files::check_cancel(cancel)?;
        let entry = entry?;
        let id = entry.file_name().to_string_lossy().into_owned();
        if !id.starts_with("m-") {
            continue;
        }
        validate_id(&id)?;
        files::safe_path(&entry.path())?;
        let mut m: ManagedModel = files::read_json(&entry.path().join("model.json"))?;
        let path = entry.path().join("model.gguf");
        if m.id != id || m.sha256 != id[2..] || m.path != relative(paths, &path)? {
            return Err(InferenceError::Integrity(
                "managed model metadata path/digest mismatch".into(),
            ));
        }
        m.availability = if !path.exists() {
            Availability::Missing
        } else {
            match files::digest(&path, cancel) {
                Ok((digest, size))
                    if digest == m.sha256
                        && size == m.size_bytes
                        && gguf::inspect(&path).is_ok() =>
                {
                    Availability::Available
                }
                Err(InferenceError::Cancelled) => return Err(InferenceError::Cancelled),
                _ => Availability::Corrupt,
            }
        };
        models.push(m);
    }
    models.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(models)
}
pub(crate) fn import(
    paths: &DrivePaths,
    source: &Path,
    name: Option<&str>,
    cancel: &CancelToken,
) -> Result<ManagedModel, InferenceError> {
    files::check_cancel(cancel)?;
    let metadata = files::regular(source)?;
    let header = gguf::inspect(source)?;
    let name = name
        .map(str::to_owned)
        .or_else(|| source.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "Model".into());
    if name.trim().is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
        return Err(InferenceError::Invalid(
            "model name must be 1-128 printable bytes".into(),
        ));
    }
    let existing = list(paths)?;
    if existing.iter().any(|m| m.name.eq_ignore_ascii_case(&name)) {
        return Err(InferenceError::Conflict(
            "a model with that name is already installed".into(),
        ));
    }
    let parent = paths.models();
    files::free_space(&parent, metadata.len())?;
    let stage = files::Stage::new(&parent)?;
    let (digest, size) = files::copy(source, &stage.0.join("model.gguf"), cancel)?;
    let checksum = source.with_extension("gguf.sha256");
    if checksum.exists() {
        if files::regular(&checksum)?.len() > 4096 {
            return Err(InferenceError::Integrity(
                "checksum sidecar too large".into(),
            ));
        }
        let text = fs::read_to_string(&checksum)?;
        let expected = text.split_whitespace().next().unwrap_or("");
        if expected.len() != 64
            || !expected.bytes().all(|b| b.is_ascii_hexdigit())
            || !expected.eq_ignore_ascii_case(&digest)
        {
            return Err(InferenceError::Integrity(
                "model checksum does not match source sidecar".into(),
            ));
        }
    }
    // Validate the copied bytes as well as the source; no incomplete install is published.
    let copied = gguf::inspect(&stage.0.join("model.gguf"))?;
    if copied.version != header.version || copied.context != header.context {
        return Err(InferenceError::Integrity(
            "GGUF metadata changed during import".into(),
        ));
    }
    let id = format!("m-{digest}");
    let target = parent.join(&id);
    if target.exists() {
        return Err(InferenceError::Conflict(
            "this model digest is already installed".into(),
        ));
    }
    let model = ManagedModel {
        id,
        name,
        path: relative(paths, &target.join("model.gguf"))?,
        sha256: digest,
        size_bytes: size,
        gguf_version: copied.version,
        context_window: copied.context,
        availability: Availability::Available,
    };
    files::write_json(&stage.0.join("model.json"), &model)?;
    files::check_cancel(cancel)?;
    files::rename(&stage.0, &target)?;
    Ok(model)
}
pub(crate) fn remove(paths: &DrivePaths, id: &str) -> Result<(), InferenceError> {
    validate_id(id)?;
    let model = list(paths)?
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| InferenceError::NotFound("model is not installed".into()))?;
    let dir = paths.models().join(&model.id);
    files::safe_path(&dir)?;
    for item in fs::read_dir(&dir)? {
        let item = item?;
        if item.file_name() != "model.gguf" && item.file_name() != "model.json" {
            return Err(InferenceError::Integrity(
                "unexpected files in managed model directory".into(),
            ));
        }
    }
    files::remove_flat(&dir)
}
