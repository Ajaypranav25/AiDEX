//! Path canonicalization and containment checks for mounts and working directories.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::SandboxError;

/// Canonicalize an existing absolute directory and strip Windows verbatim prefixes.
pub(crate) fn canonical_dir(path: &Path, what: &str) -> Result<PathBuf, SandboxError> {
    if !path.is_absolute() {
        return Err(SandboxError::invalid(format!(
            "{what} must be an absolute path: {}",
            path.display()
        )));
    }
    let canonical = fs::canonicalize(path).map_err(|error| {
        SandboxError::invalid(format!(
            "{what} {} is not accessible: {error}",
            path.display()
        ))
    })?;
    let metadata = fs::metadata(&canonical).map_err(|error| {
        SandboxError::invalid(format!(
            "{what} {} is not accessible: {error}",
            path.display()
        ))
    })?;
    if !metadata.is_dir() {
        return Err(SandboxError::invalid(format!(
            "{what} {} is not a directory",
            path.display()
        )));
    }
    Ok(simplify(canonical))
}

/// Canonicalize an existing file path.
pub(crate) fn canonical_file(path: &Path) -> Result<PathBuf, SandboxError> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| SandboxError::ProgramNotFound(format!("{}: {error}", path.display())))?;
    match fs::metadata(&canonical) {
        Ok(metadata) if metadata.is_file() => Ok(simplify(canonical)),
        Ok(_) => Err(SandboxError::ProgramNotFound(format!(
            "{} is not a file",
            path.display()
        ))),
        Err(error) => Err(SandboxError::ProgramNotFound(format!(
            "{}: {error}",
            path.display()
        ))),
    }
}

/// Remove a `\\?\` prefix from local drive paths so programs such as `cmd.exe`, which
/// reject verbatim working directories, behave normally. UNC and device paths keep it.
pub(crate) fn simplify(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(text) = path.to_str()
            && let Some(rest) = text.strip_prefix(r"\\?\")
        {
            let bytes = rest.as_bytes();
            let is_drive_path = bytes.len() >= 3
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && bytes[2] == b'\\';
            if is_drive_path && rest.len() < 260 {
                return PathBuf::from(rest);
            }
        }
        path
    }
    #[cfg(not(windows))]
    {
        path
    }
}

/// Component-wise containment (`child` equals or is beneath `parent`).
pub(crate) fn is_within(child: &Path, parent: &Path) -> bool {
    #[cfg(windows)]
    {
        // Canonical paths share the on-disk case, but drive letters may differ in case.
        let child = child.to_string_lossy().to_ascii_lowercase();
        let parent = parent.to_string_lossy().to_ascii_lowercase();
        Path::new(&child).starts_with(Path::new(&parent))
    }
    #[cfg(not(windows))]
    {
        child.starts_with(parent)
    }
}

/// Require valid UTF-8 for paths sent to the helper.
pub(crate) fn utf8(path: &Path, what: &str) -> Result<String, SandboxError> {
    path.to_str().map(str::to_owned).ok_or_else(|| {
        SandboxError::invalid(format!("{what} must be valid UTF-8: {}", path.display()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn containment_is_component_wise() {
        let root = std::env::temp_dir();
        let inside = root.join("a").join("b");
        let sibling = PathBuf::from(format!("{}-other", root.join("a").display()));
        assert!(is_within(&inside, &root.join("a")));
        assert!(is_within(&root.join("a"), &root.join("a")));
        assert!(!is_within(&sibling, &root.join("a")));
        assert!(!is_within(&root, &root.join("a")));
    }

    #[test]
    fn canonical_dir_rejects_relative_missing_and_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        assert!(canonical_dir(Path::new("relative"), "cwd").is_err());
        assert!(canonical_dir(&temp.path().join("missing"), "cwd").is_err());
        let file = temp.path().join("file.txt");
        fs::write(&file, b"x").expect("write");
        assert!(canonical_dir(&file, "cwd").is_err());
        let canonical = canonical_dir(temp.path(), "cwd").expect("canonical");
        assert!(canonical.is_absolute());
        #[cfg(windows)]
        assert!(!canonical.to_string_lossy().starts_with(r"\\?\"));
    }
}
