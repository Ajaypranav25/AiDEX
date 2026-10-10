//! CycloneDX 1.5 software bills of materials (`docs/sbom.cdx.json` in every bundle).
//!
//! The packager generates the document from `cargo metadata` and `Cargo.lock`; this
//! module is its data model and the validator used by tests, `aidex doctor`, and the
//! updater (which copies the SBOM to `manifests/<version-dir>/sbom.cdx.json`). The
//! model accepts unknown fields so that documents from other generators can be read;
//! [`Sbom::validate`] checks the fields AiDEX relies on.

use std::collections::BTreeSet;

use aidex_core::Timestamp;
use serde::{Deserialize, Serialize};

/// File name of the SBOM.
pub const SBOM_FILE_NAME: &str = "sbom.cdx.json";
/// Bundle-relative path of the SBOM (role `doc`).
pub const SBOM_BUNDLE_PATH: &str = "docs/sbom.cdx.json";
/// The CycloneDX specification version AiDEX writes.
pub const CYCLONEDX_SPEC_VERSION: &str = "1.5";

const HASH_ALGORITHMS: [(&str, usize); 6] = [
    ("MD5", 32),
    ("SHA-1", 40),
    ("SHA-256", 64),
    ("SHA-384", 96),
    ("SHA-512", 128),
    ("SHA3-256", 64),
];

/// A CycloneDX BOM (the subset AiDEX writes and checks).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sbom {
    /// Always `CycloneDX`.
    pub bom_format: String,
    /// Always [`CYCLONEDX_SPEC_VERSION`].
    pub spec_version: String,
    /// BOM revision, starting at 1.
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    pub metadata: Metadata,
    #[serde(default)]
    pub components: Vec<Component>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<Dependency>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Metadata {
    /// RFC 3339 UTC (the bundle's `created_at`).
    pub timestamp: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Tools>,
    /// The described product (the AiDEX bundle itself).
    pub component: Component,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<Property>,
}

/// CycloneDX 1.5 `metadata.tools` object form.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Tools {
    #[serde(default)]
    pub components: Vec<Component>,
}

/// Component types AiDEX uses (CycloneDX allows more; unknown ones are kept as text).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentType {
    Application,
    Framework,
    Library,
    Container,
    Platform,
    OperatingSystem,
    Device,
    DeviceDriver,
    Firmware,
    File,
    MachineLearningModel,
    Data,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Component {
    #[serde(rename = "type")]
    pub kind: ComponentType,
    #[serde(rename = "bom-ref", default, skip_serializing_if = "Option::is_none")]
    pub bom_ref: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purl: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hashes: Vec<Hash>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub licenses: Vec<LicenseChoice>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<Property>,
}

impl Component {
    /// A component with only the required fields.
    pub fn new(kind: ComponentType, name: impl Into<String>) -> Self {
        Self {
            kind,
            bom_ref: None,
            name: name.into(),
            version: None,
            description: None,
            purl: None,
            hashes: Vec::new(),
            licenses: Vec::new(),
            properties: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Hash {
    /// `SHA-256`, ...
    pub alg: String,
    /// Lowercase hex digest.
    pub content: String,
}

impl Hash {
    pub fn sha256(content: impl Into<String>) -> Self {
        Self {
            alg: "SHA-256".into(),
            content: content.into(),
        }
    }
}

/// A license as an SPDX expression or a single license id/name.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LicenseChoice {
    Expression { expression: String },
    License { license: License },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct License {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Property {
    pub name: String,
    pub value: String,
}

impl Property {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Dependency {
    #[serde(rename = "ref")]
    pub reference: String,
    #[serde(rename = "dependsOn", default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
}

/// Counts from a valid SBOM.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SbomSummary {
    pub spec_version: String,
    /// `name@version` of the described product.
    pub subject: String,
    pub components: usize,
    pub libraries: usize,
    pub files: usize,
    /// Components with at least one hash.
    pub hashed: usize,
    /// Components with license information.
    pub licensed: usize,
}

impl Sbom {
    /// Parse a CycloneDX JSON document.
    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(bytes).map_err(|error| format!("not a CycloneDX document: {error}"))
    }

    /// Pretty JSON with a trailing newline (deterministic for equal documents).
    pub fn to_json(&self) -> Result<Vec<u8>, String> {
        let mut bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    /// Check the CycloneDX 1.5 fields AiDEX relies on: format and spec version,
    /// timestamp, a named and versioned subject, unique `bom-ref`s, well-formed purls
    /// (`pkg:cargo/<name>@<version>` agreeing with the component), hash algorithms and
    /// digest lengths, non-empty license entries, and dependency references that
    /// resolve.
    pub fn validate(&self) -> Result<SbomSummary, String> {
        if self.bom_format != "CycloneDX" {
            return Err(format!("bomFormat is `{}`, not CycloneDX", self.bom_format));
        }
        if self.spec_version != CYCLONEDX_SPEC_VERSION {
            return Err(format!(
                "specVersion is `{}`, expected {CYCLONEDX_SPEC_VERSION}",
                self.spec_version
            ));
        }
        if self.version == 0 {
            return Err("version must be at least 1".into());
        }
        Timestamp::new(self.metadata.timestamp.clone())
            .map_err(|_| "metadata.timestamp is not an RFC 3339 UTC timestamp".to_owned())?;
        let subject = &self.metadata.component;
        let subject_version = subject
            .version
            .as_deref()
            .ok_or("metadata.component has no version")?;
        let mut refs = BTreeSet::new();
        validate_component(subject, &mut refs).map_err(|e| format!("metadata.component: {e}"))?;
        if let Some(tools) = &self.metadata.tools {
            for tool in &tools.components {
                if tool.name.trim().is_empty() {
                    return Err("a metadata tool has no name".into());
                }
            }
        }
        let mut summary = SbomSummary {
            spec_version: self.spec_version.clone(),
            subject: format!("{}@{subject_version}", subject.name),
            components: self.components.len(),
            libraries: 0,
            files: 0,
            hashed: 0,
            licensed: 0,
        };
        for component in &self.components {
            validate_component(component, &mut refs)
                .map_err(|error| format!("component `{}`: {error}", component.name))?;
            match component.kind {
                ComponentType::Library => summary.libraries += 1,
                ComponentType::File => summary.files += 1,
                _ => {}
            }
            summary.hashed += usize::from(!component.hashes.is_empty());
            summary.licensed += usize::from(!component.licenses.is_empty());
        }
        for dependency in &self.dependencies {
            for reference in std::iter::once(&dependency.reference).chain(&dependency.depends_on) {
                if !refs.contains(reference) {
                    return Err(format!(
                        "dependency reference `{reference}` is not a bom-ref"
                    ));
                }
            }
        }
        Ok(summary)
    }
}

fn validate_component(component: &Component, refs: &mut BTreeSet<String>) -> Result<(), String> {
    if component.name.trim().is_empty() {
        return Err("name is empty".into());
    }
    if let Some(bom_ref) = &component.bom_ref
        && (bom_ref.is_empty() || !refs.insert(bom_ref.clone()))
    {
        return Err(format!("bom-ref `{bom_ref}` is empty or not unique"));
    }
    if let Some(purl) = &component.purl {
        validate_purl(purl, component)?;
    }
    for hash in &component.hashes {
        let length = HASH_ALGORITHMS
            .iter()
            .find(|(alg, _)| *alg == hash.alg)
            .map(|(_, length)| *length)
            .ok_or_else(|| format!("unsupported hash algorithm `{}`", hash.alg))?;
        let hex = hash.content.len() == length
            && hash
                .content
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        if !hex {
            return Err(format!(
                "{} digest `{}` is malformed",
                hash.alg, hash.content
            ));
        }
    }
    for license in &component.licenses {
        let named = match license {
            LicenseChoice::Expression { expression } => !expression.trim().is_empty(),
            LicenseChoice::License { license } => license
                .id
                .as_deref()
                .or(license.name.as_deref())
                .is_some_and(|text| !text.trim().is_empty()),
        };
        if !named {
            return Err("a license entry is empty".into());
        }
    }
    Ok(())
}

fn validate_purl(purl: &str, component: &Component) -> Result<(), String> {
    let rest = purl
        .strip_prefix("pkg:")
        .ok_or_else(|| format!("purl `{purl}` does not start with `pkg:`"))?;
    let (kind, rest) = rest
        .split_once('/')
        .ok_or_else(|| format!("purl `{purl}` has no type"))?;
    if kind.is_empty() || !kind.bytes().all(|b| b.is_ascii_lowercase() || b == b'.') {
        return Err(format!("purl `{purl}` has an invalid type"));
    }
    if kind == "cargo" {
        let coordinates = rest.split(['?', '#']).next().unwrap_or(rest);
        let expected = format!(
            "{}@{}",
            component.name,
            component.version.as_deref().unwrap_or_default()
        );
        if coordinates != expected {
            return Err(format!(
                "purl `{purl}` does not name {expected} (pkg:cargo/<name>@<version>)"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library(name: &str, version: &str) -> Component {
        Component {
            bom_ref: Some(format!("pkg:cargo/{name}@{version}")),
            version: Some(version.into()),
            purl: Some(format!("pkg:cargo/{name}@{version}")),
            hashes: vec![Hash::sha256("a".repeat(64))],
            licenses: vec![LicenseChoice::Expression {
                expression: "MIT OR Apache-2.0".into(),
            }],
            ..Component::new(ComponentType::Library, name)
        }
    }

    fn sbom() -> Sbom {
        let mut subject = Component::new(ComponentType::Application, "aidex");
        subject.version = Some("0.1.0".into());
        subject.bom_ref = Some("aidex@0.1.0".into());
        Sbom {
            bom_format: "CycloneDX".into(),
            spec_version: CYCLONEDX_SPEC_VERSION.into(),
            version: 1,
            serial_number: None,
            metadata: Metadata {
                timestamp: "2026-10-10T00:00:00Z".into(),
                tools: None,
                component: subject,
                properties: Vec::new(),
            },
            components: vec![library("serde", "1.0.0"), library("serde_json", "1.0.1")],
            dependencies: vec![Dependency {
                reference: "aidex@0.1.0".into(),
                depends_on: vec!["pkg:cargo/serde@1.0.0".into()],
            }],
        }
    }

    #[test]
    fn a_well_formed_document_validates_and_round_trips() {
        let document = sbom();
        let summary = document.validate().unwrap();
        assert_eq!(summary.subject, "aidex@0.1.0");
        assert_eq!((summary.components, summary.libraries), (2, 2));
        assert_eq!((summary.hashed, summary.licensed), (2, 2));
        let json: serde_json::Value = serde_json::from_slice(&document.to_json().unwrap()).unwrap();
        assert_eq!(json["bomFormat"], "CycloneDX");
        assert_eq!(json["components"][0]["bom-ref"], "pkg:cargo/serde@1.0.0");
        assert_eq!(
            Sbom::from_json(&document.to_json().unwrap()).unwrap(),
            document
        );
    }

    #[test]
    fn missing_or_malformed_required_fields_are_rejected() {
        let mut cases: Vec<(&str, Sbom)> = Vec::new();
        let mut bad = sbom();
        bad.bom_format = "SPDX".into();
        cases.push(("format", bad));
        let mut bad = sbom();
        bad.spec_version = "1.4".into();
        cases.push(("spec", bad));
        let mut bad = sbom();
        bad.metadata.timestamp = "yesterday".into();
        cases.push(("timestamp", bad));
        let mut bad = sbom();
        bad.metadata.component.version = None;
        cases.push(("subject version", bad));
        let mut bad = sbom();
        bad.components[1].purl = Some("pkg:cargo/other@1.0.1".into());
        cases.push(("purl mismatch", bad));
        let mut bad = sbom();
        bad.components[0].hashes[0].content = "abc".into();
        cases.push(("hash", bad));
        let mut bad = sbom();
        bad.components[1].bom_ref = bad.components[0].bom_ref.clone();
        cases.push(("duplicate ref", bad));
        let mut bad = sbom();
        bad.dependencies[0]
            .depends_on
            .push("pkg:cargo/ghost@0.0.1".into());
        cases.push(("dangling dependency", bad));
        let mut bad = sbom();
        bad.components[0].licenses = vec![LicenseChoice::Expression {
            expression: " ".into(),
        }];
        cases.push(("license", bad));
        for (name, document) in cases {
            assert!(document.validate().is_err(), "{name}");
        }
        assert!(Sbom::from_json(b"{\"bomFormat\":\"CycloneDX\"}").is_err());
    }
}
