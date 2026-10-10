//! `aidex-packager build`: release binaries → verified bundle → archive + SHA256SUMS.

use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

use aidex_bundle::{
    BundleFile, BundleManifest, FileRole, MANIFEST_FILE_NAME, MANIFEST_SCHEMA_VERSION,
    PRODUCT_NAME, SHA256SUMS_FILE_NAME, Sha256Sums, StatePaths, Target, VerifyScope,
    archive::{ArchiveFormat, archive_file_name, bundle_root_name, write_archive},
    layout, validate_bundle_path, verify_bundle,
};
use aidex_core::Timestamp;
use serde::Serialize;

use crate::{
    cli::BuildArgs,
    error::PackagerError,
    licenses,
    workspace::{GitInfo, Metadata, rustc_version},
};

/// A shipped binary: cargo package, cargo binary name, bundle stem, and role.
struct Shipped {
    package: &'static str,
    binary: &'static str,
    stem: &'static str,
    role: FileRole,
}

const SHIPPED: [Shipped; 4] = [
    Shipped {
        package: "aidex-launcher",
        binary: "aidex-launcher",
        stem: layout::LAUNCHER_BINARY,
        role: FileRole::Launcher,
    },
    Shipped {
        package: "aidex-cli",
        binary: "aidex",
        stem: layout::CLI_BINARY,
        role: FileRole::Cli,
    },
    Shipped {
        package: "aidexd",
        binary: "aidexd",
        stem: layout::DAEMON_BINARY,
        role: FileRole::Daemon,
    },
    Shipped {
        package: "aidex-sandbox",
        binary: "aidex-sandbox-exec",
        stem: layout::HELPER_BINARY,
        role: FileRole::Helper,
    },
];

#[derive(Debug, Serialize)]
pub struct Timings {
    pub cargo_build_ms: u128,
    pub assemble_ms: u128,
    pub archive_ms: u128,
    pub total_ms: u128,
}

#[derive(Debug, Serialize)]
pub struct BuildReport {
    pub target: Target,
    pub rust_target: String,
    pub version: String,
    pub git_commit: Option<String>,
    pub source_dirty: Option<bool>,
    pub created_at: String,
    pub bundle_dir: PathBuf,
    pub bundle_bytes: u64,
    pub files: usize,
    pub licenses: usize,
    pub missing_license_texts: Vec<String>,
    pub archive: PathBuf,
    pub archive_bytes: u64,
    pub archive_sha256: String,
    pub sha256sums: PathBuf,
    pub timings: Timings,
}

pub fn run(args: &BuildArgs) -> Result<BuildReport, PackagerError> {
    let started = Instant::now();
    let target = args.target;
    let triple = args
        .triple
        .clone()
        .unwrap_or_else(|| target.default_rust_triple().to_owned());
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let metadata = Metadata::load(&cargo, args.manifest_path.as_deref(), &triple)?;
    let workspace = metadata.workspace_root.clone();
    let version = metadata.member("aidex-cli")?.version.clone();
    let git = GitInfo::read(&workspace);
    let created_at = build_time(&git)?;
    let target_dir = args
        .target_dir
        .clone()
        .unwrap_or_else(|| metadata.target_directory.join("packager"));

    let cargo_started = Instant::now();
    cargo_build(
        &cargo,
        &workspace,
        target,
        &triple,
        &target_dir,
        &created_at,
    )?;
    let cargo_build_ms = cargo_started.elapsed().as_millis();

    let assemble_started = Instant::now();
    fs::create_dir_all(&args.out).map_err(|error| {
        PackagerError::io(
            format_args!("cannot create `{}`", args.out.display()),
            error,
        )
    })?;
    let out = std::path::absolute(&args.out).map_err(|error| {
        PackagerError::io(
            format_args!("cannot resolve `{}`", args.out.display()),
            error,
        )
    })?;
    let root_name = bundle_root_name(&version, target);
    let staging = out.join(format!(".{root_name}.staging-{}", std::process::id()));
    remove_own_output(&staging)?;
    let release = target_dir.join(&triple).join("release");
    let roots: Vec<&str> = SHIPPED.iter().map(|shipped| shipped.package).collect();
    let inventory = licenses::collect(&metadata, &roots)?;
    let assembled = assemble(&staging, &release, target, &inventory, args);
    let files = match assembled {
        Ok(files) => files,
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
    };
    let mut manifest = BundleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        product: PRODUCT_NAME.into(),
        version: version.clone(),
        git_commit: git.commit.clone(),
        source_dirty: git.dirty,
        target,
        rust_target: triple.clone(),
        rustc: rustc_version(&workspace),
        created_at: created_at.clone(),
        min_drive_schema: aidex_drive::CURRENT_SCHEMA_VERSION,
        state: StatePaths::current(),
        files,
        licenses: inventory.entries.clone(),
    };
    manifest.normalize();
    let bytes = manifest
        .to_json()
        .map_err(|error| PackagerError::new(format!("generated manifest is invalid: {error}")))?;
    fs::write(staging.join(MANIFEST_FILE_NAME), bytes)
        .map_err(|error| PackagerError::io("cannot write bundle.json", error))?;
    verify_bundle(&staging, VerifyScope::All)?;
    let bundle_dir = out.join(&root_name);
    remove_own_output(&bundle_dir)?;
    fs::rename(&staging, &bundle_dir).map_err(|error| {
        PackagerError::io(
            format_args!("cannot move bundle to `{}`", bundle_dir.display()),
            error,
        )
    })?;
    let bundle = verify_bundle(&bundle_dir, VerifyScope::All)?;
    let assemble_ms = assemble_started.elapsed().as_millis();

    let archive_started = Instant::now();
    let archive_path = out.join(archive_file_name(&version, target));
    let summary = write_archive(&bundle, &archive_path, ArchiveFormat::for_target(target))?;
    let sums_path = out.join(SHA256SUMS_FILE_NAME);
    let mut sums = Sha256Sums::load(&sums_path)?;
    let archive_name = archive_file_name(&version, target);
    sums.set(&archive_name, &summary.sha256)
        .map_err(PackagerError::new)?;
    sums.store(&sums_path)?;
    let archive_ms = archive_started.elapsed().as_millis();

    Ok(BuildReport {
        target,
        rust_target: triple,
        version,
        git_commit: git.commit,
        source_dirty: git.dirty,
        created_at: created_at.as_str().to_owned(),
        bundle_bytes: bundle.manifest.files.iter().map(|file| file.size).sum(),
        files: bundle.manifest.files.len(),
        licenses: bundle.manifest.licenses.len(),
        missing_license_texts: inventory.missing_texts,
        bundle_dir,
        archive: summary.path,
        archive_bytes: summary.size,
        archive_sha256: summary.sha256,
        sha256sums: sums_path,
        timings: Timings {
            cargo_build_ms,
            assemble_ms,
            archive_ms,
            total_ms: started.elapsed().as_millis(),
        },
    })
}

/// `SOURCE_DATE_EPOCH`, else the `HEAD` commit time, else now (whole seconds).
fn build_time(git: &GitInfo) -> Result<Timestamp, PackagerError> {
    let seconds = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .or(git.commit_time);
    let millis = match seconds {
        Some(seconds) => seconds.saturating_mul(1000),
        None => Timestamp::now().unix_millis().div_euclid(1000) * 1000,
    };
    Timestamp::from_unix_millis(millis)
        .map_err(|error| PackagerError::new(format!("invalid build time: {error}")))
}

/// `cargo build --release --locked` of exactly the shipped binaries.
fn cargo_build(
    cargo: &OsString,
    workspace: &Path,
    target: Target,
    triple: &str,
    target_dir: &Path,
    created_at: &Timestamp,
) -> Result<(), PackagerError> {
    let mut command = Command::new(cargo);
    command
        .current_dir(workspace)
        .args([
            "build",
            "--release",
            "--locked",
            "--target",
            triple,
            "--target-dir",
        ])
        .arg(target_dir);
    for shipped in &SHIPPED {
        command.args(["-p", shipped.package]);
    }
    for shipped in &SHIPPED {
        command.args(["--bin", shipped.binary]);
    }
    command
        .env(
            "CARGO_ENCODED_RUSTFLAGS",
            rustflags(workspace, target_dir, target).join("\u{1f}"),
        )
        .env("CARGO_PROFILE_RELEASE_STRIP", "symbols")
        .env(
            "SOURCE_DATE_EPOCH",
            created_at.unix_millis().div_euclid(1000).to_string(),
        );
    if matches!(target, Target::MacosArm64 | Target::MacosX64) {
        command.env("MACOSX_DEPLOYMENT_TARGET", "11.0");
    }
    let status = command
        .status()
        .map_err(|error| PackagerError::io("cannot run cargo build", error))?;
    if !status.success() {
        return Err(PackagerError::new(format!(
            "cargo build --release --target {triple} failed ({status}); build each target on its own OS, with `rustup target add {triple}` (and musl-tools for musl targets)"
        )));
    }
    Ok(())
}

/// Packaging rustflags: static CRT and deterministic PE timestamps on Windows, and
/// local paths (workspace, cargo home, target directory) remapped out of the binaries.
fn rustflags(workspace: &Path, target_dir: &Path, target: Target) -> Vec<String> {
    let mut flags = Vec::new();
    if target.is_windows() {
        flags.push("-Ctarget-feature=+crt-static".to_owned());
        flags.push("-Clink-arg=/Brepro".to_owned());
    }
    flags.push(format!(
        "--remap-path-prefix={}=/aidex",
        workspace.display()
    ));
    if let Some(cargo_home) = cargo_home() {
        flags.push(format!(
            "--remap-path-prefix={}=/cargo",
            cargo_home.display()
        ));
    }
    // Build-script output (`OUT_DIR`) lives under the target directory.
    let target_dir = std::path::absolute(target_dir).unwrap_or_else(|_| target_dir.to_path_buf());
    flags.push(format!(
        "--remap-path-prefix={}=/target",
        target_dir.display()
    ));
    flags
}

fn cargo_home() -> Option<PathBuf> {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(|home| PathBuf::from(home).join(".cargo"))
        })
}

/// Copy binaries, license texts, and optional components into `staging`.
fn assemble(
    staging: &Path,
    release: &Path,
    target: Target,
    inventory: &licenses::Inventory,
    args: &BuildArgs,
) -> Result<Vec<BundleFile>, PackagerError> {
    let bin = staging.join(layout::BIN_DIR);
    fs::create_dir_all(&bin).map_err(|error| {
        PackagerError::io(format_args!("cannot create `{}`", bin.display()), error)
    })?;
    let mut files = Vec::new();
    for shipped in &SHIPPED {
        let built = release.join(target.executable_name(shipped.binary));
        let path = layout::bin_path(shipped.stem, target);
        copy(&built, &staging.join(&path))?;
        files.push(BundleFile::describe(staging, &path, shipped.role, true)?);
    }
    licenses::write_texts(staging, &inventory.texts)?;
    for text in &inventory.texts {
        files.push(BundleFile::describe(
            staging,
            &text.path,
            FileRole::Doc,
            false,
        )?);
    }
    for (source, prefix, role) in [
        (&args.desktop, "desktop", FileRole::Desktop),
        (&args.installer, "installer", FileRole::Installer),
    ] {
        if let Some(source) = source {
            add_component(staging, source, prefix, role, &mut files)?;
        }
    }
    Ok(files)
}

/// Copy a prebuilt optional app (file or directory tree) under `prefix/`.
fn add_component(
    staging: &Path,
    source: &Path,
    prefix: &str,
    role: FileRole,
    files: &mut Vec<BundleFile>,
) -> Result<(), PackagerError> {
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| PackagerError::new(format!("`{}` has no usable name", source.display())))?;
    let mut pending = vec![(source.to_path_buf(), format!("{prefix}/{name}"))];
    while let Some((path, relative)) = pending.pop() {
        validate_bundle_path(&relative).map_err(PackagerError::new)?;
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            PackagerError::io(format_args!("cannot read `{}`", path.display()), error)
        })?;
        if metadata.file_type().is_symlink() {
            return Err(PackagerError::new(format!(
                "`{}` is a symbolic link; bundles cannot contain links",
                path.display()
            )));
        }
        if metadata.is_dir() {
            let entries = fs::read_dir(&path).map_err(|error| {
                PackagerError::io(format_args!("cannot list `{}`", path.display()), error)
            })?;
            for entry in entries {
                let entry = entry.map_err(|error| {
                    PackagerError::io(format_args!("cannot list `{}`", path.display()), error)
                })?;
                let child = entry
                    .file_name()
                    .to_str()
                    .map(str::to_owned)
                    .ok_or_else(|| {
                        PackagerError::new(format!(
                            "`{}` has a non-UTF-8 name",
                            entry.path().display()
                        ))
                    })?;
                pending.push((entry.path(), format!("{relative}/{child}")));
            }
            continue;
        }
        let destination = relative
            .split('/')
            .fold(staging.to_path_buf(), |path, part| path.join(part));
        copy(&path, &destination)?;
        files.push(BundleFile::describe(
            staging,
            &relative,
            role,
            is_executable(&path, &metadata),
        )?);
    }
    Ok(())
}

fn is_executable(path: &Path, metadata: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = path;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        path.extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    }
}

fn copy(source: &Path, destination: &Path) -> Result<(), PackagerError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            PackagerError::io(format_args!("cannot create `{}`", parent.display()), error)
        })?;
    }
    fs::copy(source, destination).map(drop).map_err(|error| {
        PackagerError::io(
            format_args!(
                "cannot copy `{}` to `{}`",
                source.display(),
                destination.display()
            ),
            error,
        )
    })
}

/// Remove a previous packager output directory; refuse anything that is not one.
fn remove_own_output(dir: &Path) -> Result<(), PackagerError> {
    match fs::symlink_metadata(dir) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(PackagerError::io(
            format_args!("cannot inspect `{}`", dir.display()),
            error,
        )),
        Ok(metadata) => {
            let ours = metadata.is_dir()
                && !metadata.file_type().is_symlink()
                && (dir.join(MANIFEST_FILE_NAME).is_file()
                    || dir
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().contains(".staging-")));
            if !ours {
                return Err(PackagerError::new(format!(
                    "`{}` exists and is not a packager bundle directory; move it aside",
                    dir.display()
                )));
            }
            fs::remove_dir_all(dir).map_err(|error| {
                PackagerError::io(format_args!("cannot replace `{}`", dir.display()), error)
            })
        }
    }
}
