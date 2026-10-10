//! CycloneDX 1.5 SBOM of a bundle, from `cargo metadata`, `Cargo.lock`, and the
//! bundle's own binaries.
//!
//! Components: every crate linked into the shipped binaries (the same normal-dependency
//! closure as the license inventory) with name, version, purl (`pkg:cargo/<name>@<version>`
//! for crates.io; `repository_url`/`vcs_url` qualifiers for other registries and git;
//! none for workspace crates, which are not published), the declared SPDX expression,
//! and the `Cargo.lock` SHA-256 of the `.crate` archive where one is recorded; plus one
//! `file` component per critical bundle binary with its SHA-256. `dependencies` is the
//! linked dependency graph. Output is deterministic (sorted, timestamp = `created_at`).

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use aidex_bundle::{BundleFile, Target};
use aidex_core::Timestamp;
use aidex_updater::sbom::{
    CYCLONEDX_SPEC_VERSION, Component, ComponentType, Dependency, Hash, LicenseChoice, Metadata,
    Property, Sbom, Tools,
};
use serde::Deserialize;

use crate::{
    error::PackagerError,
    licenses::shipped_packages,
    workspace::{Metadata as CargoMetadata, Package},
};

const CRATES_IO: &str = "registry+https://github.com/rust-lang/crates.io-index";

/// Facts about the bundle the SBOM describes.
pub struct Subject<'a> {
    pub version: &'a str,
    pub target: Target,
    pub rust_target: &'a str,
    pub git_commit: Option<&'a str>,
    pub source_dirty: Option<bool>,
    pub rustc: Option<&'a str>,
    pub created_at: &'a Timestamp,
}

#[derive(Deserialize)]
struct Lockfile {
    #[serde(default)]
    package: Vec<LockedPackage>,
}

#[derive(Deserialize)]
struct LockedPackage {
    name: String,
    version: String,
    source: Option<String>,
    checksum: Option<String>,
}

/// `(name, version, source)` → SHA-256 checksum recorded in `Cargo.lock`.
pub fn lock_checksums(
    workspace: &Path,
) -> Result<BTreeMap<(String, String, String), String>, PackagerError> {
    let path = workspace.join("Cargo.lock");
    let text = fs::read_to_string(&path).map_err(|error| {
        PackagerError::io(format_args!("cannot read `{}`", path.display()), error)
    })?;
    let lock: Lockfile = toml::from_str(&text).map_err(|error| {
        PackagerError::new(format!("cannot parse `{}`: {error}", path.display()))
    })?;
    Ok(lock
        .package
        .into_iter()
        .filter_map(|package| {
            let source = package.source?;
            let checksum = package.checksum?;
            Some(((package.name, package.version, source), checksum))
        })
        .collect())
}

/// The SBOM for `subject`, whose shipped binaries are `files`, built from `roots`.
pub fn generate(
    metadata: &CargoMetadata,
    roots: &[&str],
    checksums: &BTreeMap<(String, String, String), String>,
    subject: &Subject<'_>,
    files: &[BundleFile],
) -> Result<Sbom, PackagerError> {
    let packages = shipped_packages(metadata, roots)?;
    let refs: BTreeMap<&str, String> = packages
        .iter()
        .map(|package| (package.id.as_str(), bom_ref(metadata, package)))
        .collect();
    let mut components = Vec::new();
    for package in &packages {
        components.push(crate_component(metadata, package, &refs, checksums));
    }
    for file in files.iter().filter(|file| file.role.is_critical()) {
        let mut component = Component::new(ComponentType::File, file.path.clone());
        component.bom_ref = Some(format!("file:{}", file.path));
        component.hashes = vec![Hash::sha256(file.sha256.clone())];
        component.properties = vec![Property::new("aidex:role", file.role.as_str())];
        components.push(component);
    }
    components.sort_by(|a, b| a.bom_ref.cmp(&b.bom_ref));

    let subject_ref = format!("aidex@{}", subject.version);
    let mut dependencies = vec![Dependency {
        reference: subject_ref.clone(),
        depends_on: roots
            .iter()
            .filter_map(|root| metadata.member(root).ok())
            .filter_map(|package| refs.get(package.id.as_str()).cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    }];
    if let Some(resolve) = &metadata.resolve {
        let mut nodes: Vec<_> = resolve
            .nodes
            .iter()
            .filter(|node| refs.contains_key(node.id.as_str()))
            .collect();
        nodes.sort_by(|a, b| refs[a.id.as_str()].cmp(&refs[b.id.as_str()]));
        for node in nodes {
            let depends_on: BTreeSet<String> = node
                .deps
                .iter()
                .filter(|dep| {
                    dep.dep_kinds.is_empty() || dep.dep_kinds.iter().any(|kind| kind.kind.is_none())
                })
                .filter_map(|dep| refs.get(dep.pkg.as_str()).cloned())
                .collect();
            dependencies.push(Dependency {
                reference: refs[node.id.as_str()].clone(),
                depends_on: depends_on.into_iter().collect(),
            });
        }
    }

    let mut product = Component::new(ComponentType::Application, "aidex");
    product.version = Some(subject.version.to_owned());
    product.bom_ref = Some(subject_ref);
    product.description = Some(format!("AiDEX portable bundle for {}", subject.target));
    product.licenses = vec![LicenseChoice::Expression {
        expression: "MIT".into(),
    }];
    let mut properties = vec![
        Property::new("aidex:target", subject.target.as_str()),
        Property::new("aidex:rust_target", subject.rust_target),
    ];
    if let Some(commit) = subject.git_commit {
        properties.push(Property::new("aidex:git_commit", commit));
    }
    if let Some(dirty) = subject.source_dirty {
        properties.push(Property::new("aidex:source_dirty", dirty.to_string()));
    }
    if let Some(rustc) = subject.rustc {
        properties.push(Property::new("aidex:rustc", rustc));
    }
    product.properties = properties;
    let mut tool = Component::new(ComponentType::Application, "aidex-packager");
    tool.version = Some(env!("CARGO_PKG_VERSION").into());
    Ok(Sbom {
        bom_format: "CycloneDX".into(),
        spec_version: CYCLONEDX_SPEC_VERSION.into(),
        version: 1,
        serial_number: None,
        metadata: Metadata {
            timestamp: subject.created_at.as_str().to_owned(),
            tools: Some(Tools {
                components: vec![tool],
            }),
            component: product,
            properties: Vec::new(),
        },
        components,
        dependencies,
    })
}

fn crate_component(
    metadata: &CargoMetadata,
    package: &Package,
    refs: &BTreeMap<&str, String>,
    checksums: &BTreeMap<(String, String, String), String>,
) -> Component {
    let mut component = Component::new(ComponentType::Library, package.name.clone());
    component.version = Some(package.version.clone());
    component.bom_ref = refs.get(package.id.as_str()).cloned();
    component.purl = purl(metadata, package);
    if let Some(source) = &package.source {
        if let Some(checksum) = checksums.get(&(
            package.name.clone(),
            package.version.clone(),
            source.clone(),
        )) {
            component.hashes = vec![Hash::sha256(checksum.clone())];
        }
    }
    let license = if metadata.is_member(package) {
        Some(package.license.clone().unwrap_or_else(|| "MIT".into()))
    } else {
        package
            .license
            .clone()
            .filter(|license| !license.trim().is_empty())
    };
    if let Some(license) = license {
        component.licenses = vec![LicenseChoice::Expression {
            expression: spdx_expression(&license),
        }];
    }
    let source = if metadata.is_member(package) {
        "workspace".to_owned()
    } else {
        package.source.clone().unwrap_or_else(|| "path".into())
    };
    component.properties = vec![Property::new("aidex:source", source)];
    component
}

fn purl(metadata: &CargoMetadata, package: &Package) -> Option<String> {
    if metadata.is_member(package) {
        return None;
    }
    let base = format!("pkg:cargo/{}@{}", package.name, package.version);
    let source = package.source.as_deref()?;
    if source == CRATES_IO {
        return Some(base);
    }
    if let Some(url) = source.strip_prefix("registry+") {
        return Some(format!("{base}?repository_url={}", percent_encode(url)));
    }
    if let Some(url) = source.strip_prefix("sparse+") {
        return Some(format!("{base}?repository_url={}", percent_encode(url)));
    }
    if let Some(url) = source.strip_prefix("git+") {
        let url = url.split('#').next().unwrap_or(url);
        return Some(format!(
            "{base}?vcs_url={}",
            percent_encode(&format!("git+{url}"))
        ));
    }
    None
}

fn bom_ref(metadata: &CargoMetadata, package: &Package) -> String {
    purl(metadata, package).unwrap_or_else(|| {
        let origin = if metadata.is_member(package) {
            "workspace"
        } else {
            "path"
        };
        format!("{origin}:{}@{}", package.name, package.version)
    })
}

/// Cargo's legacy `MIT/Apache-2.0` means `MIT OR Apache-2.0` in SPDX.
fn spdx_expression(license: &str) -> String {
    license
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// Percent-encode a purl qualifier value (RFC 3986 unreserved characters stay).
fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use aidex_bundle::FileRole;

    fn metadata(root: &Path) -> CargoMetadata {
        let package = |name: &str, license: Option<&str>, source: Option<&str>| {
            serde_json::json!({
                "name": name,
                "version": "1.0.0",
                "id": format!("{name} 1.0.0"),
                "license": license,
                "license_file": null,
                "source": source,
                "manifest_path": root.join(name).join("Cargo.toml"),
            })
        };
        let json = serde_json::json!({
            "packages": [
                package("app", None, None),
                package("linked", Some("MIT/Apache-2.0"), Some(CRATES_IO)),
                package("forked", Some("BSD-3-Clause"), Some("git+https://example.com/forked.git#abc")),
                package("builder", Some("MIT"), Some(CRATES_IO)),
            ],
            "resolve": { "nodes": [
                { "id": "app 1.0.0", "deps": [
                    { "pkg": "linked 1.0.0", "dep_kinds": [{ "kind": null }] },
                    { "pkg": "forked 1.0.0", "dep_kinds": [{ "kind": null }] },
                    { "pkg": "builder 1.0.0", "dep_kinds": [{ "kind": "build" }] },
                ]},
                { "id": "linked 1.0.0", "deps": [] },
                { "id": "forked 1.0.0", "deps": [] },
                { "id": "builder 1.0.0", "deps": [] },
            ]},
            "workspace_members": ["app 1.0.0"],
            "workspace_root": root,
            "target_directory": root.join("target"),
        });
        CargoMetadata::parse(json.to_string().as_bytes()).unwrap()
    }

    #[test]
    fn the_sbom_lists_linked_crates_binaries_licenses_and_hashes_and_validates() {
        let root = tempfile::tempdir().unwrap();
        let metadata = metadata(root.path());
        let mut checksums = BTreeMap::new();
        checksums.insert(
            (
                "linked".to_owned(),
                "1.0.0".to_owned(),
                CRATES_IO.to_owned(),
            ),
            "b".repeat(64),
        );
        let created_at = Timestamp::new("2026-10-10T00:00:00Z").unwrap();
        let subject = Subject {
            version: "0.1.0",
            target: Target::LinuxX64,
            rust_target: "x86_64-unknown-linux-musl",
            git_commit: Some(&"a".repeat(40)),
            source_dirty: Some(false),
            rustc: Some("rustc 1.95.0"),
            created_at: &created_at,
        };
        let files = vec![BundleFile {
            path: "bin/aidex".into(),
            sha256: "c".repeat(64),
            size: 1,
            executable: true,
            role: FileRole::Cli,
        }];
        let sbom = generate(&metadata, &["app"], &checksums, &subject, &files).unwrap();
        let summary = sbom.validate().unwrap();
        assert_eq!(summary.subject, "aidex@0.1.0");
        assert_eq!((summary.libraries, summary.files), (3, 1));
        let by_name: BTreeMap<_, _> = sbom
            .components
            .iter()
            .map(|component| (component.name.as_str(), component))
            .collect();
        assert!(
            !by_name.contains_key("builder"),
            "build deps are not shipped"
        );
        assert_eq!(
            by_name["linked"].purl.as_deref(),
            Some("pkg:cargo/linked@1.0.0")
        );
        assert_eq!(by_name["linked"].hashes, [Hash::sha256("b".repeat(64))]);
        assert_eq!(
            by_name["linked"].licenses,
            [LicenseChoice::Expression {
                expression: "MIT OR Apache-2.0".into()
            }]
        );
        assert_eq!(
            by_name["forked"].purl.as_deref(),
            Some("pkg:cargo/forked@1.0.0?vcs_url=git%2Bhttps%3A%2F%2Fexample.com%2Fforked.git")
        );
        assert_eq!(by_name["app"].purl, None);
        assert_eq!(
            by_name["app"].bom_ref.as_deref(),
            Some("workspace:app@1.0.0")
        );
        assert_eq!(by_name["bin/aidex"].hashes, [Hash::sha256("c".repeat(64))]);
        let app_deps = sbom
            .dependencies
            .iter()
            .find(|dep| dep.reference == "workspace:app@1.0.0")
            .unwrap();
        assert_eq!(app_deps.depends_on.len(), 2);
        // Deterministic.
        let again = generate(&metadata, &["app"], &checksums, &subject, &files).unwrap();
        assert_eq!(sbom.to_json().unwrap(), again.to_json().unwrap());
    }
}
