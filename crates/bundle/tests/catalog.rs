mod common;
use aidex_bundle::{BundleFile, FileRole, desktop_catalog};

#[test]
fn a_cli_only_bundle_cannot_be_presented_as_a_desktop_installation() {
    let (_temp, source) = common::bundle(common::host(), "0.1.0", "catalog");
    assert!(
        desktop_catalog(&source)
            .unwrap_err()
            .to_string()
            .contains("no executable AiDEX desktop")
    );
}

#[test]
fn catalog_metadata_comes_from_the_verified_distribution_and_rejects_tampering() {
    let (_temp, source) = common::bundle(common::host(), "0.1.0", "catalog");
    std::fs::create_dir(source.join("desktop")).unwrap();
    let path = format!(
        "desktop/{}",
        common::host().executable_name("aidex-desktop")
    );
    std::fs::write(source.join(&path), b"synthetic desktop fixture").unwrap();
    let (mut manifest, _) = aidex_bundle::read_manifest(&source).unwrap();
    manifest
        .files
        .push(BundleFile::describe(&source, &path, FileRole::Desktop, true).unwrap());
    manifest.normalize();
    common::write_manifest(&source, &manifest);
    let catalog = desktop_catalog(&source).unwrap();
    assert_eq!(catalog.version, "0.1.0");
    assert_eq!(catalog.file_count, 6);
    assert!(catalog.required_free_bytes > catalog.installed_bytes);
    std::fs::write(source.join(&path), b"modified desktop fixture").unwrap();
    assert!(desktop_catalog(&source).is_err());
}
