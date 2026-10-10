//! The pinned runtime catalog (`inference/runtimes.toml`, compiled in).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{InferenceError, Result, fsutil};

/// The repository's `inference/runtimes.toml`.
pub const BUILTIN_CATALOG: &str = include_str!("../../../../inference/runtimes.toml");
pub const CATALOG_VERSION: u32 = 1;

/// How a runtime build executes models.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeVariant {
    Cpu,
    Cuda,
    Vulkan,
    Metal,
}

impl RuntimeVariant {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Cuda => "cuda",
            Self::Vulkan => "vulkan",
            Self::Metal => "metal",
        }
    }

    pub const fn is_gpu(self) -> bool {
        !matches!(self, Self::Cpu)
    }
}

impl fmt::Display for RuntimeVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArchiveFormat {
    Zip,
    TarGz,
}

impl ArchiveFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::TarGz => "tar-gz",
        }
    }
}

/// One pinned upstream release archive.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogEntry {
    pub id: String,
    pub engine: String,
    pub version: String,
    /// `aidex_drive::platform_runtime_name` value (`windows-x64`, ...).
    pub platform: String,
    pub variant: RuntimeVariant,
    pub archive: String,
    pub format: ArchiveFormat,
    pub size_bytes: u64,
    pub sha256: String,
    pub url: String,
    pub source: String,
    pub license: String,
    /// Path of the server executable inside the archive (`/` separators).
    pub server_binary: String,
    /// Hash of the extracted file set (see the catalog header), when measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree_sha256: Option<String>,
    /// AiDEX ran its real-model test with this archive.
    pub validated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl CatalogEntry {
    /// `<version>-<variant>`, the install directory name.
    pub fn install_name(&self) -> String {
        format!("{}-{}", self.version, self.variant)
    }

    fn validate(&self) -> std::result::Result<(), String> {
        let safe = |text: &str| {
            !text.is_empty()
                && text.len() <= 128
                && text
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        };
        if !safe(&self.id) || !safe(&self.version) || !safe(&self.platform) {
            return Err(format!(
                "runtime `{}` has an unsafe id, version, or platform",
                self.id
            ));
        }
        if !fsutil::is_sha256_hex(&self.sha256)
            || self
                .tree_sha256
                .as_deref()
                .is_some_and(|tree| !fsutil::is_sha256_hex(tree))
        {
            return Err(format!("runtime `{}` has an invalid SHA-256", self.id));
        }
        if self.server_binary.is_empty()
            || self
                .server_binary
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(format!("runtime `{}` has an unsafe server_binary", self.id));
        }
        if !self.url.starts_with("https://") {
            return Err(format!("runtime `{}` must be fetched over https", self.id));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCatalog {
    pub version: u32,
    #[serde(rename = "runtime", default)]
    pub runtimes: Vec<CatalogEntry>,
}

impl RuntimeCatalog {
    /// The catalog compiled into this build.
    pub fn builtin() -> Result<Self> {
        Self::parse(BUILTIN_CATALOG)
    }

    pub fn parse(text: &str) -> Result<Self> {
        let catalog: Self = toml::from_str(text).map_err(|error| {
            InferenceError::corrupt("inference/runtimes.toml", error.message().to_owned())
        })?;
        if catalog.version != CATALOG_VERSION {
            return Err(InferenceError::corrupt(
                "inference/runtimes.toml",
                format!("catalog version {} is not supported", catalog.version),
            ));
        }
        for (index, entry) in catalog.runtimes.iter().enumerate() {
            entry
                .validate()
                .map_err(|reason| InferenceError::corrupt("inference/runtimes.toml", reason))?;
            if catalog.runtimes[..index].iter().any(|other| {
                other.id == entry.id || other.sha256.eq_ignore_ascii_case(&entry.sha256)
            }) {
                return Err(InferenceError::corrupt(
                    "inference/runtimes.toml",
                    format!("runtime `{}` is listed twice", entry.id),
                ));
            }
        }
        Ok(catalog)
    }

    /// A catalog of exactly `entries` (tests and embedders).
    pub fn from_entries(runtimes: Vec<CatalogEntry>) -> Result<Self> {
        let text = toml::to_string(&Self {
            version: CATALOG_VERSION,
            runtimes,
        })
        .map_err(|error| InferenceError::InvalidArgument(error.to_string()))?;
        Self::parse(&text)
    }

    pub fn find(&self, id: &str) -> Option<&CatalogEntry> {
        self.runtimes.iter().find(|entry| entry.id == id)
    }

    pub fn find_by_sha256(&self, sha256: &str) -> Option<&CatalogEntry> {
        self.runtimes
            .iter()
            .find(|entry| entry.sha256.eq_ignore_ascii_case(sha256))
    }

    pub fn for_platform<'a>(&'a self, platform: &'a str) -> impl Iterator<Item = &'a CatalogEntry> {
        self.runtimes
            .iter()
            .filter(move |entry| entry.platform == platform)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_pins_the_validated_windows_cpu_runtime() {
        let catalog = RuntimeCatalog::builtin().unwrap();
        let entry = catalog.find("llama.cpp-b11205-windows-x64-cpu").unwrap();
        assert_eq!(entry.archive, "llama-b11205-bin-win-cpu-x64.zip");
        assert_eq!(
            entry.sha256,
            "2a8bf1ff808e8f6d2329f50f7005419a9930d9ecfbeae5047cfff475e87c2db0"
        );
        assert_eq!(entry.size_bytes, 19_155_045);
        assert_eq!((entry.license.as_str(), entry.validated), ("MIT", true));
        assert_eq!(entry.format, ArchiveFormat::Zip);
        assert_eq!(entry.install_name(), "b11205-cpu");
        assert!(
            catalog
                .find_by_sha256(&entry.sha256.to_uppercase())
                .is_some()
        );
        assert!(catalog.for_platform("macos-arm64").count() >= 1);
        // Only validated archives claim validation.
        assert_eq!(
            catalog
                .runtimes
                .iter()
                .filter(|entry| entry.validated)
                .count(),
            1
        );
    }

    #[test]
    fn invalid_catalogs_are_rejected() {
        let mut entry = RuntimeCatalog::builtin().unwrap().runtimes[0].clone();
        entry.server_binary = "../evil".into();
        assert!(RuntimeCatalog::from_entries(vec![entry.clone()]).is_err());
        entry.server_binary = "llama-server.exe".into();
        entry.url = "http://example.invalid/x.zip".into();
        assert!(RuntimeCatalog::from_entries(vec![entry.clone()]).is_err());
        entry.url = "https://example.invalid/x.zip".into();
        assert!(RuntimeCatalog::from_entries(vec![entry.clone(), entry.clone()]).is_err());
        assert!(RuntimeCatalog::parse("version = 2").is_err());
        assert!(RuntimeCatalog::parse("version = 1\n[[runtime]]\nid = \"x\"\nbogus = 1").is_err());
    }
}
