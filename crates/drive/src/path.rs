//! Validated, slash-normalized paths relative to a drive root.

use std::{
    error::Error,
    fmt,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RelativePath(String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PathError {
    Empty,
    Absolute,
    ParentTraversal,
    InvalidComponent,
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "path must not be empty",
            Self::Absolute => "path must be relative",
            Self::ParentTraversal => "parent traversal is not allowed",
            Self::InvalidComponent => "path contains an unsupported component",
        })
    }
}

impl Error for PathError {}

impl RelativePath {
    /// Parse slash-separated path segments, rejecting traversal and platform roots.
    pub fn new(path: impl AsRef<str>) -> Result<Self, PathError> {
        let path = path.as_ref();
        if path.is_empty() {
            return Err(PathError::Empty);
        }
        let parsed = Path::new(path);
        if parsed.is_absolute() || path.starts_with('/') || path.starts_with('\\') {
            return Err(PathError::Absolute);
        }
        let mut parts = Vec::new();
        for part in path.split(['/', '\\']) {
            match part {
                "" | "." => continue,
                ".." => return Err(PathError::ParentTraversal),
                segment if segment.contains(':') => return Err(PathError::InvalidComponent),
                segment => parts.push(segment),
            }
        }
        if parts.is_empty() {
            return Err(PathError::Empty);
        }
        Ok(Self(parts.join("/")))
    }

    /// Wrap a compile-time path literal that is already normalized and safe.
    pub(crate) fn trusted(path: &str) -> Self {
        debug_assert_eq!(Self::new(path).as_ref().map(Self::as_str), Ok(path));
        Self(path.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Resolve beneath a root after lexical validation; does not follow symlinks.
    pub fn resolve_under(&self, root: impl AsRef<Path>) -> PathBuf {
        let mut result = root.as_ref().to_path_buf();
        for component in Path::new(&self.0).components() {
            if let Component::Normal(segment) = component {
                result.push(segment);
            }
        }
        result
    }
}

impl Serialize for RelativePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RelativePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let path = String::deserialize(deserializer)?;
        Self::new(path).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::{PathError, RelativePath};
    use std::path::Path;

    #[test]
    fn normalizes_separators_and_resolves_beneath_root() {
        let path = RelativePath::new(r"projects\alpha/./src").unwrap();

        assert_eq!(path.as_str(), "projects/alpha/src");
        assert_eq!(
            path.resolve_under("drive"),
            Path::new("drive")
                .join("projects")
                .join("alpha")
                .join("src")
        );
    }

    #[test]
    fn rejects_absolute_and_parent_traversal_paths() {
        assert_eq!(RelativePath::new("/etc/passwd"), Err(PathError::Absolute));
        assert!(RelativePath::new(r"C:\secrets").is_err());
        assert_eq!(
            RelativePath::new("workspace/../secrets"),
            Err(PathError::ParentTraversal)
        );
    }
}
