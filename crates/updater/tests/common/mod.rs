//! Signed synthetic bundles, update packages, and drives for updater tests. Every
//! signing key is generated at run time; none is stored anywhere.

#![allow(dead_code)]

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use aidex_bundle::{
    BundleFile, BundleManifest, CurrentPointer, FileRole, KeyPair, KeySource, LAUNCHER_MARKER,
    LicenseEntry, MANIFEST_FILE_NAME, MANIFEST_SCHEMA_VERSION, PRODUCT_NAME, SHA256SUMS_FILE_NAME,
    Sha256Sums, SignaturePolicy, StatePaths, Target, TrustStore, VerifyScope,
    archive::{ArchiveFormat, archive_file_name, bundle_root_name, write_archive},
    layout, read_manifest_bytes, resolve_active, sign_manifest, verify_bundle, write_signature,
};
use aidex_core::Timestamp;
use aidex_drive::DrivePaths;
use aidex_events::MemoryEventSink;
use aidex_updater::{
    SelfTestFn, SelfTestReport, UpdateError, UpdateSource, Updater, UpdaterConfig,
    sbom::{
        CYCLONEDX_SPEC_VERSION, Component, ComponentType, Hash, LicenseChoice, Metadata,
        SBOM_BUNDLE_PATH, Sbom,
    },
};

pub const TEST_BIN: &str = env!("CARGO_BIN_EXE_aidex-updater-test-bin");

/// The host's bundle target (tests only run on named targets).
pub fn host() -> Target {
    Target::host().expect("tests run on a named bundle target")
}

pub fn other_target() -> Target {
    if host() == Target::LinuxX64 {
        Target::WindowsX64
    } else {
        Target::LinuxX64
    }
}

/// How a synthetic bundle is built.
#[derive(Clone)]
pub struct Spec {
    pub target: Target,
    pub version: String,
    /// Makes otherwise identical builds differ.
    pub tag: String,
    /// Use the test binary for `aidex`/`aidexd` (for the real process self-test).
    pub real_binaries: bool,
    pub sbom: bool,
    pub min_drive_schema: u32,
}

impl Spec {
    pub fn new(version: &str) -> Self {
        Self {
            target: host(),
            version: version.into(),
            tag: "a".into(),
            real_binaries: false,
            sbom: true,
            min_drive_schema: aidex_drive::CURRENT_SCHEMA_VERSION,
        }
    }

    pub fn target(mut self, target: Target) -> Self {
        self.target = target;
        self
    }

    pub fn tag(mut self, tag: &str) -> Self {
        self.tag = tag.into();
        self
    }

    pub fn real_binaries(mut self) -> Self {
        self.real_binaries = true;
        self
    }

    pub fn min_drive_schema(mut self, schema: u32) -> Self {
        self.min_drive_schema = schema;
        self
    }
}

/// Write an unsigned synthetic bundle for `spec` into `dir`.
pub fn write_bundle(dir: &Path, spec: &Spec) -> BundleManifest {
    let target = spec.target;
    fs::create_dir_all(dir.join("bin")).unwrap();
    fs::create_dir_all(dir.join("licenses/aidex")).unwrap();
    let mut files = Vec::new();
    for (role, stem) in [
        (FileRole::Launcher, layout::LAUNCHER_BINARY),
        (FileRole::Cli, layout::CLI_BINARY),
        (FileRole::Daemon, layout::DAEMON_BINARY),
        (FileRole::Helper, layout::HELPER_BINARY),
    ] {
        let path = layout::bin_path(stem, target);
        let real = spec.real_binaries && matches!(role, FileRole::Cli | FileRole::Daemon);
        if real {
            fs::copy(TEST_BIN, dir.join(&path)).unwrap();
        } else {
            let marker = if role == FileRole::Launcher {
                LAUNCHER_MARKER
            } else {
                ""
            };
            fs::write(
                dir.join(&path),
                format!("{stem} {} {} {target}\n{marker}\n", spec.version, spec.tag),
            )
            .unwrap();
        }
        files.push(BundleFile::describe(dir, &path, role, true).unwrap());
    }
    let license = "licenses/aidex/LICENSE";
    fs::write(dir.join(license), "MIT License\n").unwrap();
    files.push(BundleFile::describe(dir, license, FileRole::Doc, false).unwrap());
    let created_at = Timestamp::from_unix_millis(1_760_000_000_000).unwrap();
    if spec.sbom {
        fs::create_dir_all(dir.join("docs")).unwrap();
        let sbom = sbom_for(&spec.version, &created_at, &files);
        fs::write(dir.join(SBOM_BUNDLE_PATH), sbom.to_json().unwrap()).unwrap();
        files.push(BundleFile::describe(dir, SBOM_BUNDLE_PATH, FileRole::Doc, false).unwrap());
    }
    let mut manifest = BundleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        product: PRODUCT_NAME.into(),
        version: spec.version.clone(),
        git_commit: Some("0".repeat(40)),
        source_dirty: Some(false),
        target,
        rust_target: target.default_rust_triple().into(),
        rustc: Some("rustc 1.95.0 (test)".into()),
        created_at,
        min_drive_schema: spec.min_drive_schema,
        state: StatePaths::current(),
        files,
        licenses: vec![LicenseEntry {
            component: "aidex-cli".into(),
            version: spec.version.clone(),
            spdx: "MIT".into(),
            source: "workspace".into(),
            path: Some(license.into()),
        }],
    };
    manifest.normalize();
    fs::write(dir.join(MANIFEST_FILE_NAME), manifest.to_json().unwrap()).unwrap();
    manifest
}

fn sbom_for(version: &str, created_at: &Timestamp, files: &[BundleFile]) -> Sbom {
    let mut subject = Component::new(ComponentType::Application, "aidex");
    subject.version = Some(version.into());
    subject.bom_ref = Some(format!("aidex@{version}"));
    let mut library = Component::new(ComponentType::Library, "serde");
    library.version = Some("1.0.0".into());
    library.purl = Some("pkg:cargo/serde@1.0.0".into());
    library.bom_ref = library.purl.clone();
    library.licenses = vec![LicenseChoice::Expression {
        expression: "MIT OR Apache-2.0".into(),
    }];
    let mut components = vec![library];
    for file in files.iter().filter(|file| file.role.is_critical()) {
        let mut component = Component::new(ComponentType::File, file.path.clone());
        component.hashes = vec![Hash::sha256(file.sha256.clone())];
        components.push(component);
    }
    Sbom {
        bom_format: "CycloneDX".into(),
        spec_version: CYCLONEDX_SPEC_VERSION.into(),
        version: 1,
        serial_number: None,
        metadata: Metadata {
            timestamp: created_at.as_str().to_owned(),
            tools: None,
            component: subject,
            properties: Vec::new(),
        },
        components,
        dependencies: Vec::new(),
    }
}

pub fn sign(dir: &Path, key: &KeyPair) {
    let bytes = read_manifest_bytes(dir).unwrap();
    write_signature(dir, &sign_manifest(&bytes, key)).unwrap();
}

/// A drive, a signing key trusted by the updater, captured events, and a scratch
/// directory for packages.
pub struct Fixture {
    pub drive: tempfile::TempDir,
    pub out: tempfile::TempDir,
    pub key: KeyPair,
    pub events: Arc<MemoryEventSink>,
}

impl Fixture {
    pub fn new() -> Self {
        let drive = tempfile::tempdir().unwrap();
        aidex_drive::init_with(
            drive.path(),
            aidex_drive::InitOptions {
                require_removable: false,
            },
        )
        .unwrap();
        Self {
            drive,
            out: tempfile::tempdir().unwrap(),
            key: KeyPair::generate("updater-tests").unwrap(),
            events: Arc::new(MemoryEventSink::new()),
        }
    }

    pub fn root(&self) -> &Path {
        self.drive.path()
    }

    pub fn paths(&self) -> DrivePaths {
        DrivePaths::open(self.root()).unwrap()
    }

    pub fn trust(&self) -> TrustStore {
        TrustStore::empty().with_key(self.key.trusted_key(KeySource::Explicit))
    }

    /// The updater as the daemon would build it, with a self-test that only checks
    /// that the staged CLI exists (synthetic binaries cannot run).
    pub fn updater(&self) -> Updater {
        let self_test = SelfTestFn(|dir: &Path, manifest: &BundleManifest, _drive: &Path| {
            let cli = manifest.file(FileRole::Cli).expect("cli");
            if dir.join(&cli.path).is_file() {
                Ok(SelfTestReport::default())
            } else {
                Err(UpdateError::SelfTestFailed {
                    command: "aidex --version".into(),
                    detail: "missing".into(),
                })
            }
        });
        let config = UpdaterConfig::new(self.trust()).with_self_test(Arc::new(self_test));
        self.updater_with(config)
    }

    pub fn updater_with(&self, config: UpdaterConfig) -> Updater {
        Updater::new(self.paths(), host(), config).with_events(self.events.clone())
    }

    /// A fresh unpacked bundle directory under `out/<name>/`.
    pub fn bundle_dir(&self, name: &str, spec: &Spec, signed: bool) -> PathBuf {
        let dir = self
            .out
            .path()
            .join(name)
            .join(bundle_root_name(&spec.version, spec.target));
        write_bundle(&dir, spec);
        if signed {
            sign(&dir, &self.key);
        }
        dir
    }

    /// A signed package archive (with a SHA256SUMS entry) in `out/<name>/`.
    pub fn package(&self, name: &str, spec: &Spec) -> PathBuf {
        let dir = self.bundle_dir(&format!("{name}-src"), spec, true);
        self.archive(name, &dir, spec)
    }

    /// Archive a (verifiable) bundle directory into `out/<name>/` with SHA256SUMS.
    pub fn archive(&self, name: &str, dir: &Path, spec: &Spec) -> PathBuf {
        let folder = self.out.path().join(name);
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join(archive_file_name(&spec.version, spec.target));
        let bundle = verify_bundle(dir, VerifyScope::All).unwrap();
        let summary =
            write_archive(&bundle, &path, ArchiveFormat::for_target(spec.target)).unwrap();
        let sums_path = folder.join(SHA256SUMS_FILE_NAME);
        let mut sums = Sha256Sums::load(&sums_path).unwrap();
        sums.set(path.file_name().unwrap().to_str().unwrap(), &summary.sha256)
            .unwrap();
        sums.store(&sums_path).unwrap();
        path
    }

    /// Apply a signed `version` package and return the archive.
    pub fn install(&self, version: &str) -> PathBuf {
        let package = self.package(&format!("pkg-{version}"), &Spec::new(version));
        self.updater()
            .apply(
                &UpdateSource::LocalFile(package.clone()),
                &Default::default(),
            )
            .unwrap();
        package
    }

    pub fn pointer(&self) -> Option<CurrentPointer> {
        CurrentPointer::load(&layout::pointer_path(&self.paths())).unwrap()
    }

    pub fn pointer_bytes(&self) -> Option<Vec<u8>> {
        fs::read(layout::pointer_path(&self.paths())).ok()
    }

    /// The version the launcher would run (full verification with the test key).
    pub fn active_version(&self) -> Option<String> {
        let policy = SignaturePolicy::require(self.trust());
        resolve_active(&self.paths(), host(), VerifyScope::All, &policy)
            .ok()
            .map(|active| active.bundle.manifest.version)
    }

    pub fn version_dirs(&self) -> Vec<String> {
        list(&self.root().join("app").join("versions"))
    }

    pub fn staging_entries(&self) -> Vec<String> {
        list(&self.root().join("app").join("staging"))
    }

    /// Files directly in `app/` (pointer, journal, lock, temporaries).
    pub fn app_files(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.root().join("app"))
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_type().unwrap().is_file())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    pub fn event_names(&self) -> Vec<&'static str> {
        self.events.names()
    }
}

fn list(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// Archive every file under `dir` (no verification), as `root/<relative>`, in the
/// format `out`'s extension names. For tampered or incomplete packages.
pub fn raw_archive(dir: &Path, root: &str, out: &Path) {
    let mut files = Vec::new();
    collect(dir, "", &mut files);
    files.sort();
    let name = out.file_name().unwrap().to_string_lossy().into_owned();
    if name.ends_with(".zip") {
        let mut writer = zip::ZipWriter::new(fs::File::create(out).unwrap());
        writer
            .add_directory(format!("{root}/"), zip::write::SimpleFileOptions::default())
            .unwrap();
        for relative in files {
            writer
                .start_file(
                    format!("{root}/{relative}"),
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            writer
                .write_all(&fs::read(dir.join(&relative)).unwrap())
                .unwrap();
        }
        writer.finish().unwrap();
    } else {
        let encoder = flate2::write::GzEncoder::new(
            fs::File::create(out).unwrap(),
            flate2::Compression::fast(),
        );
        let mut builder = tar::Builder::new(encoder);
        for relative in files {
            let bytes = fs::read(dir.join(&relative)).unwrap();
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_entry_type(tar::EntryType::Regular);
            builder
                .append_data(&mut header, format!("{root}/{relative}"), &bytes[..])
                .unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();
    }
}

/// A zip with arbitrary entry names (hostile archives).
pub fn zip_entries(out: &Path, entries: &[(&str, &[u8])]) {
    let mut writer = zip::ZipWriter::new(fs::File::create(out).unwrap());
    for (name, data) in entries {
        writer
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(data).unwrap();
    }
    writer.finish().unwrap();
}

fn collect(dir: &Path, prefix: &str, files: &mut Vec<String>) {
    for entry in fs::read_dir(if prefix.is_empty() {
        dir.to_path_buf()
    } else {
        dir.join(prefix)
    })
    .unwrap()
    {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        let relative = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if entry.file_type().unwrap().is_dir() {
            collect(dir, &relative, files);
        } else {
            files.push(relative);
        }
    }
}
