//! License inventory of everything compiled into the shipped binaries.
//!
//! The inventory is the normal-dependency closure of the shipped packages for the
//! target platform, taken from `cargo metadata`; build and dev dependencies are not
//! linked into the binaries and are excluded. For each crate the declared SPDX
//! expression is recorded and its license files (`LICENSE*`, `COPYING*`, `NOTICE*`, ...)
//! are collected into `licenses/third-party/<name>-<version>.txt`. This is license
//! metadata, not a full SBOM (no hashes of sources, no CycloneDX); see issue #27.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
};

use aidex_bundle::LicenseEntry;

use crate::{
    error::PackagerError,
    workspace::{Metadata, Package},
};

/// Bundle-relative path of the AiDEX license (shared by every workspace crate).
pub const AIDEX_LICENSE_PATH: &str = "licenses/aidex/LICENSE";
const THIRD_PARTY_DIR: &str = "licenses/third-party";
const LICENSE_PREFIXES: [&str; 6] = [
    "LICENSE",
    "LICENCE",
    "COPYING",
    "NOTICE",
    "COPYRIGHT",
    "UNLICENSE",
];

/// A license text file to write into the bundle.
#[derive(Debug, Eq, PartialEq)]
pub struct LicenseText {
    /// Bundle-relative path.
    pub path: String,
    pub contents: Vec<u8>,
}

/// The inventory and the texts to write.
#[derive(Debug, Default)]
pub struct Inventory {
    pub entries: Vec<LicenseEntry>,
    pub texts: Vec<LicenseText>,
    /// Third-party components whose packages contain no license file.
    pub missing_texts: Vec<String>,
}

/// Packages reachable from `roots` through normal (linked) dependencies.
pub fn shipped_packages<'a>(
    metadata: &'a Metadata,
    roots: &[&str],
) -> Result<Vec<&'a Package>, PackagerError> {
    let resolve = metadata
        .resolve
        .as_ref()
        .ok_or_else(|| PackagerError::new("cargo metadata has no dependency graph"))?;
    let nodes: BTreeMap<&str, _> = resolve
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::new();
    for root in roots {
        let package = metadata.member(root)?;
        if seen.insert(package.id.as_str()) {
            queue.push_back(package.id.as_str());
        }
    }
    while let Some(id) = queue.pop_front() {
        let node = nodes.get(id).ok_or_else(|| {
            PackagerError::new(format!("`{id}` is missing from the dependency graph"))
        })?;
        for dep in &node.deps {
            let linked =
                dep.dep_kinds.is_empty() || dep.dep_kinds.iter().any(|kind| kind.kind.is_none());
            if linked && seen.insert(dep.pkg.as_str()) {
                queue.push_back(dep.pkg.as_str());
            }
        }
    }
    let mut packages: Vec<&Package> = metadata
        .packages
        .iter()
        .filter(|package| seen.contains(package.id.as_str()))
        .collect();
    packages.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
    Ok(packages)
}

/// Build the license inventory for `roots`.
pub fn collect(metadata: &Metadata, roots: &[&str]) -> Result<Inventory, PackagerError> {
    let mut inventory = Inventory::default();
    let aidex_license = metadata.workspace_root.join("LICENSE");
    let aidex_text = fs::read(&aidex_license).map_err(|error| {
        PackagerError::io(
            format_args!("cannot read `{}`", aidex_license.display()),
            error,
        )
    })?;
    inventory.texts.push(LicenseText {
        path: AIDEX_LICENSE_PATH.into(),
        contents: aidex_text,
    });
    for package in shipped_packages(metadata, roots)? {
        if metadata.is_member(package) {
            inventory.entries.push(LicenseEntry {
                component: package.name.clone(),
                version: package.version.clone(),
                // Workspace crates are covered by the repository's MIT license even when
                // a crate manifest omits `license.workspace = true`.
                spdx: package.license.clone().unwrap_or_else(|| "MIT".into()),
                source: "workspace".into(),
                path: Some(AIDEX_LICENSE_PATH.into()),
            });
            continue;
        }
        let texts = license_files(package)?;
        let path = if texts.is_empty() {
            inventory
                .missing_texts
                .push(format!("{} {}", package.name, package.version));
            None
        } else {
            let path = format!("{THIRD_PARTY_DIR}/{}-{}.txt", package.name, package.version);
            inventory.texts.push(LicenseText {
                path: path.clone(),
                contents: concatenate(&texts)?,
            });
            Some(path)
        };
        inventory.entries.push(LicenseEntry {
            component: package.name.clone(),
            version: package.version.clone(),
            spdx: package
                .license
                .clone()
                .filter(|license| !license.trim().is_empty())
                .unwrap_or_else(|| "NOASSERTION".into()),
            source: package.source.clone().unwrap_or_else(|| "path".into()),
            path,
        });
    }
    Ok(inventory)
}

/// License-like files at the top of a package directory, plus `license-file`.
fn license_files(package: &Package) -> Result<Vec<PathBuf>, PackagerError> {
    let dir = package.manifest_path.parent().ok_or_else(|| {
        PackagerError::new(format!("`{}` has no package directory", package.name))
    })?;
    let mut files = BTreeSet::new();
    let entries = fs::read_dir(dir).map_err(|error| {
        PackagerError::io(format_args!("cannot list `{}`", dir.display()), error)
    })?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_ascii_uppercase();
        let is_license = LICENSE_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix));
        if is_license && entry.file_type().is_ok_and(|kind| kind.is_file()) {
            files.insert(entry.path());
        }
    }
    if let Some(declared) = &package.license_file {
        let path = dir.join(declared);
        if path.is_file() {
            files.insert(path);
        }
    }
    Ok(files.into_iter().collect())
}

fn concatenate(files: &[PathBuf]) -> Result<Vec<u8>, PackagerError> {
    let mut out = Vec::new();
    for file in files {
        let name = file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let contents = fs::read(file).map_err(|error| {
            PackagerError::io(format_args!("cannot read `{}`", file.display()), error)
        })?;
        if !out.is_empty() {
            out.push(b'\n');
        }
        out.extend_from_slice(format!("==> {name} <==\n").as_bytes());
        out.extend_from_slice(&contents);
        if !contents.ends_with(b"\n") {
            out.push(b'\n');
        }
    }
    Ok(out)
}

/// Write the collected texts under `bundle_dir`.
pub fn write_texts(bundle_dir: &Path, texts: &[LicenseText]) -> Result<(), PackagerError> {
    for text in texts {
        let path = text
            .path
            .split('/')
            .fold(bundle_dir.to_path_buf(), |path, part| path.join(part));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                PackagerError::io(format_args!("cannot create `{}`", parent.display()), error)
            })?;
        }
        fs::write(&path, &text.contents).map_err(|error| {
            PackagerError::io(format_args!("cannot write `{}`", path.display()), error)
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(root: &Path) -> Metadata {
        let package = |name: &str, license: Option<&str>, source: Option<&str>| {
            let dir = root.join(name);
            fs::create_dir_all(&dir).unwrap();
            serde_json::json!({
                "name": name,
                "version": "1.0.0",
                "id": format!("{name} 1.0.0"),
                "license": license,
                "license_file": null,
                "source": source,
                "manifest_path": dir.join("Cargo.toml"),
            })
        };
        let registry = Some("registry+https://github.com/rust-lang/crates.io-index");
        let json = serde_json::json!({
            "packages": [
                package("app", None, None),
                package("linked", Some("MIT OR Apache-2.0"), registry),
                package("transitive", Some("BSD-3-Clause"), registry),
                package("builder", Some("MIT"), registry),
                package("tester", Some("MIT"), registry),
                package("bare", None, registry),
            ],
            "resolve": { "nodes": [
                { "id": "app 1.0.0", "deps": [
                    { "pkg": "linked 1.0.0", "dep_kinds": [{ "kind": null }] },
                    { "pkg": "builder 1.0.0", "dep_kinds": [{ "kind": "build" }] },
                    { "pkg": "tester 1.0.0", "dep_kinds": [{ "kind": "dev" }] },
                    { "pkg": "bare 1.0.0", "dep_kinds": [{ "kind": null }, { "kind": "dev" }] },
                ]},
                { "id": "linked 1.0.0", "deps": [
                    { "pkg": "transitive 1.0.0", "dep_kinds": [{ "kind": null }] },
                ]},
                { "id": "transitive 1.0.0", "deps": [] },
                { "id": "builder 1.0.0", "deps": [] },
                { "id": "tester 1.0.0", "deps": [] },
                { "id": "bare 1.0.0", "deps": [] },
            ]},
            "workspace_members": ["app 1.0.0"],
            "workspace_root": root,
            "target_directory": root.join("target"),
        });
        Metadata::parse(json.to_string().as_bytes()).unwrap()
    }

    #[test]
    fn the_closure_follows_only_linked_dependencies() {
        let root = tempfile::tempdir().unwrap();
        let metadata = metadata(root.path());
        let names: Vec<_> = shipped_packages(&metadata, &["app"])
            .unwrap()
            .into_iter()
            .map(|package| package.name.as_str())
            .collect();
        assert_eq!(names, ["app", "bare", "linked", "transitive"]);
        assert!(shipped_packages(&metadata, &["missing"]).is_err());
    }

    #[test]
    fn the_inventory_records_spdx_sources_and_collected_texts() {
        let root = tempfile::tempdir().unwrap();
        let metadata = metadata(root.path());
        fs::write(root.path().join("LICENSE"), "MIT License (AiDEX)\n").unwrap();
        fs::write(root.path().join("linked/LICENSE-MIT"), "MIT text").unwrap();
        fs::write(root.path().join("linked/LICENSE-APACHE"), "Apache text\n").unwrap();
        fs::write(root.path().join("linked/README.md"), "not a license").unwrap();
        fs::write(root.path().join("transitive/COPYING"), "BSD text\n").unwrap();
        let inventory = collect(&metadata, &["app"]).unwrap();

        let by_name: BTreeMap<_, _> = inventory
            .entries
            .iter()
            .map(|entry| (entry.component.as_str(), entry))
            .collect();
        assert_eq!(by_name["app"].spdx, "MIT");
        assert_eq!(by_name["app"].source, "workspace");
        assert_eq!(by_name["app"].path.as_deref(), Some(AIDEX_LICENSE_PATH));
        assert_eq!(by_name["linked"].spdx, "MIT OR Apache-2.0");
        assert!(by_name["linked"].source.starts_with("registry+"));
        assert_eq!(by_name["bare"].spdx, "NOASSERTION");
        assert_eq!(by_name["bare"].path, None);
        assert_eq!(inventory.missing_texts, ["bare 1.0.0"]);

        let linked = inventory
            .texts
            .iter()
            .find(|text| text.path == "licenses/third-party/linked-1.0.0.txt")
            .unwrap();
        let text = String::from_utf8(linked.contents.clone()).unwrap();
        assert_eq!(
            text,
            "==> LICENSE-APACHE <==\nApache text\n\n==> LICENSE-MIT <==\nMIT text\n"
        );
        assert!(!text.contains("not a license"));
    }
}
