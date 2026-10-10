use std::{fs, path::Path};

use super::{
    RemovableStatus, VolumeInfo, probe_manifest,
    unix::{
        DiskutilInfo, classify_diskutil, classify_sysfs, is_listed_block_device, parse_df,
        parse_diskutil, parse_proc_mounts, unescape_octal,
    },
    volume_for,
    windows::{classify, parse},
};
use crate::{DriveError, InitOptions, MANIFEST_FILE_NAME, init_with};

/// `Get-Volume` output shaped like a workstation with a USB stick, a portable SSD,
/// two internal NVMe volumes, an optical drive, and an empty card reader.
const WINDOWS_ROWS: &str = r#"[
  {"letter":"H","label":"","filesystem":"NTFS","drive_type":"Removable","bus_type":"USB","size":123048275968,"free":122950373376},
  {"letter":"C","label":"Windows-SSD","filesystem":"NTFS","drive_type":"Fixed","bus_type":"NVMe","size":509722226688,"free":47837687808},
  {"letter":"T","label":"T7","filesystem":"exFAT","drive_type":"Fixed","bus_type":"USB","size":1000202039296,"free":632000000000},
  {"letter":"E","label":"Dev","filesystem":"NTFS","drive_type":"Fixed","bus_type":"NVMe","size":322123591680,"free":116065148928},
  {"letter":"R","label":"","filesystem":"","drive_type":"CD-ROM","bus_type":"SATA","size":0,"free":0},
  {"letter":"S","label":"","filesystem":"","drive_type":"Removable","bus_type":"USB","size":0,"free":0}
]"#;

fn volume(removable: RemovableStatus, system: bool) -> VolumeInfo {
    VolumeInfo {
        root: "/media/aidex".into(),
        label: None,
        filesystem: None,
        total_bytes: Some(1),
        free_bytes: Some(1),
        removable,
        system,
        has_aidex_manifest: false,
        schema_version: None,
    }
}

#[test]
fn windows_rows_classify_usb_ssds_as_portable_and_internal_disks_as_fixed() {
    let volumes = parse(WINDOWS_ROWS, Some("C:")).unwrap();
    let by_letter = |letter: &str| {
        volumes
            .iter()
            .find(|volume| volume.root == Path::new(&format!("{letter}:\\")))
            .unwrap_or_else(|| panic!("{letter}: missing"))
    };

    assert_eq!(volumes.len(), 5, "the optical drive is skipped");
    let stick = by_letter("H");
    assert_eq!(stick.removable, RemovableStatus::Removable);
    assert_eq!(stick.label, None, "an empty label is unreported");
    assert_eq!(stick.filesystem.as_deref(), Some("NTFS"));
    assert_eq!(stick.total_bytes, Some(123_048_275_968));
    assert_eq!(stick.free_bytes, Some(122_950_373_376));
    assert!(!stick.system);

    let ssd = by_letter("T");
    assert_eq!(ssd.removable, RemovableStatus::UsbFixed);
    assert_eq!(ssd.label.as_deref(), Some("T7"));

    let system = by_letter("C");
    assert_eq!(system.removable, RemovableStatus::Fixed);
    assert!(system.system);
    assert_eq!(by_letter("E").removable, RemovableStatus::Fixed);
    assert!(!by_letter("E").system);

    let reader = by_letter("S");
    assert_eq!(reader.total_bytes, None, "no media reports no capacity");
    assert_eq!(reader.free_bytes, None);
}

#[test]
fn windows_classification_accepts_display_names_and_raw_cim_numbers() {
    assert_eq!(classify("Removable", "USB"), RemovableStatus::Removable);
    assert_eq!(classify("2", ""), RemovableStatus::Removable);
    assert_eq!(classify("Fixed", "USB"), RemovableStatus::UsbFixed);
    assert_eq!(classify("3", "7"), RemovableStatus::UsbFixed);
    assert_eq!(classify("Fixed", "SD"), RemovableStatus::Removable);
    for internal in ["NVMe", "SATA", "RAID", "SAS", "Virtual", "17", "11", ""] {
        assert_eq!(
            classify("Fixed", internal),
            RemovableStatus::Fixed,
            "{internal}"
        );
    }
    assert_eq!(classify("Unknown", "USB"), RemovableStatus::Unknown);
    assert_eq!(classify("", ""), RemovableStatus::Unknown);
}

#[test]
fn windows_parse_handles_empty_output_and_rejects_garbage() {
    assert!(parse("", None).unwrap().is_empty());
    assert!(parse("\u{feff}[]\r\n", None).unwrap().is_empty());
    assert!(matches!(
        parse("not json", None),
        Err(DriveError::Discovery(_))
    ));
    let without_system = parse(WINDOWS_ROWS, None).unwrap();
    assert!(without_system.iter().all(|volume| !volume.system));
    // Rows without a single drive letter are ignored.
    let odd = r#"[{"letter":"","drive_type":"Fixed"},{"letter":"AB","drive_type":"Fixed"}]"#;
    assert!(parse(odd, None).unwrap().is_empty());
}

#[test]
fn proc_mounts_decode_escapes_and_keep_only_real_block_devices() {
    let mounts = "/dev/sda2 / ext4 rw,relatime 0 0\n\
                  proc /proc proc rw 0 0\n\
                  /dev/sdb1 /media/me/My\\040Drive exfat rw 0 0\n\
                  /dev/loop3 /snap/core/1 squashfs ro 0 0\n\
                  server:/share /mnt/share nfs rw 0 0\n";
    let entries = parse_proc_mounts(mounts);
    assert_eq!(entries.len(), 5);
    assert_eq!(entries[2].mount_point, Path::new("/media/me/My Drive"));
    assert_eq!(entries[2].filesystem, "exfat");
    let listed: Vec<_> = entries
        .iter()
        .filter(|entry| is_listed_block_device(&entry.device))
        .map(|entry| entry.device.as_str())
        .collect();
    assert_eq!(listed, ["/dev/sda2", "/dev/sdb1"]);
}

#[test]
fn df_rows_convert_kibibytes_and_keep_mount_points_with_spaces() {
    let output = "Filesystem     1024-blocks      Used Available Capacity Mounted on\n\
                  /dev/sda2         102400000  51200000  40960000      56% /\n\
                  /dev/disk4s1      976762584       100 976762484       1% /Volumes/My Drive\n\
                  garbage line\n";
    let rows = parse_df(output);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].total_bytes, 102_400_000 * 1024);
    assert_eq!(rows[0].free_bytes, 40_960_000 * 1024);
    assert_eq!(rows[1].device, "/dev/disk4s1");
    assert_eq!(rows[1].mount_point, Path::new("/Volumes/My Drive"));
}

#[test]
fn sysfs_classification_treats_usb_disks_as_portable() {
    let usb =
        "/sys/devices/pci0000:00/0000:00:14.0/usb2/2-1/2-1:1.0/host0/target0:0:0/0:0:0:0/block/sdb";
    let nvme = "/sys/devices/pci0000:00/0000:00:1d.0/0000:3d:00.0/nvme/nvme0/nvme0n1";
    assert_eq!(classify_sysfs(usb, Some(true)), RemovableStatus::Removable);
    assert_eq!(classify_sysfs(usb, Some(false)), RemovableStatus::UsbFixed);
    assert_eq!(classify_sysfs(nvme, Some(false)), RemovableStatus::Fixed);
    assert_eq!(classify_sysfs(nvme, Some(true)), RemovableStatus::Removable);
    assert_eq!(classify_sysfs(nvme, None), RemovableStatus::Unknown);
}

#[test]
fn diskutil_fixed_media_is_not_mistaken_for_removable_media() {
    let internal = "   Volume Name:               Macintosh HD\n\
                    File System Personality:   APFS\n\
                    Protocol:                  Apple Fabric\n\
                    Removable Media:           Fixed\n";
    let info = parse_diskutil(internal);
    assert_eq!(info.volume_name.as_deref(), Some("Macintosh HD"));
    assert_eq!(info.filesystem.as_deref(), Some("APFS"));
    assert_eq!(classify_diskutil(&info), RemovableStatus::Fixed);

    let ssd = "   Volume Name:               T7\n   Protocol:                  USB\n   Removable Media:           Fixed\n";
    assert_eq!(
        classify_diskutil(&parse_diskutil(ssd)),
        RemovableStatus::UsbFixed
    );

    let card =
        "   Protocol:                  Secure Digital\n   Removable Media:           Removable\n";
    assert_eq!(
        classify_diskutil(&parse_diskutil(card)),
        RemovableStatus::Removable
    );
    assert_eq!(
        classify_diskutil(&DiskutilInfo::default()),
        RemovableStatus::Unknown
    );
}

#[test]
fn escapes_decode_octal_and_hex_sequences() {
    assert_eq!(unescape_octal(r"My\040Drive"), "My Drive");
    assert_eq!(unescape_octal(r"My\x20Drive"), "My Drive");
    assert_eq!(unescape_octal(r"tab\011end"), "tab\tend");
    assert_eq!(unescape_octal(r"plain\9"), r"plain\9");
    assert_eq!(unescape_octal(r"trailing\"), r"trailing\");
}

#[test]
fn only_portable_non_system_volumes_are_eligible() {
    assert!(RemovableStatus::Removable.is_portable());
    assert!(RemovableStatus::UsbFixed.is_portable());
    assert!(!RemovableStatus::Fixed.is_portable());
    assert!(!RemovableStatus::Unknown.is_portable());
    assert!(volume(RemovableStatus::UsbFixed, false).is_eligible());
    assert!(volume(RemovableStatus::Removable, false).is_eligible());
    assert!(!volume(RemovableStatus::UsbFixed, true).is_eligible());
    assert!(!volume(RemovableStatus::Fixed, false).is_eligible());
    assert!(!volume(RemovableStatus::Unknown, false).is_eligible());
}

#[test]
fn removable_status_serializes_as_stable_snake_case() {
    for status in [
        RemovableStatus::Removable,
        RemovableStatus::UsbFixed,
        RemovableStatus::Fixed,
        RemovableStatus::Unknown,
    ] {
        assert_eq!(
            serde_json::to_value(status).unwrap(),
            serde_json::Value::String(status.as_str().into())
        );
    }
}

#[test]
fn manifest_probe_reports_presence_and_supported_schema_without_writing() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(probe_manifest(root.path()), (false, None));

    let manifest = root.path().join(MANIFEST_FILE_NAME);
    fs::write(&manifest, "not json").unwrap();
    assert_eq!(probe_manifest(root.path()), (true, None));
    assert_eq!(fs::read_to_string(&manifest).unwrap(), "not json");
    fs::remove_file(&manifest).unwrap();

    init_with(
        root.path(),
        InitOptions {
            require_removable: false,
        },
    )
    .unwrap();
    let before = fs::read(&manifest).unwrap();
    assert_eq!(probe_manifest(root.path()), (true, Some(2)));
    assert_eq!(fs::read(&manifest).unwrap(), before);

    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join(MANIFEST_FILE_NAME)).unwrap();
    assert_eq!(probe_manifest(directory.path()), (true, None));
}

#[test]
fn the_volume_holding_a_temporary_directory_is_found_and_not_eligible() {
    let root = tempfile::tempdir().unwrap();
    match volume_for(root.path()) {
        Ok(volume) => {
            let canonical = fs::canonicalize(root.path()).unwrap();
            let canonical = canonical.to_string_lossy().to_ascii_lowercase();
            let volume_root = volume.root.to_string_lossy().to_ascii_lowercase();
            assert!(
                canonical
                    .trim_start_matches(r"\\?\")
                    .starts_with(&volume_root),
                "{volume_root} does not hold {canonical}"
            );
            assert!(
                !volume.is_eligible(),
                "a temporary directory lives on an internal disk: {volume:?}"
            );
        }
        // Containers may mount `/` from an overlay that is not a listed block device.
        Err(error) => {
            if cfg!(windows) {
                panic!("volume lookup failed on Windows: {error}");
            }
            assert!(matches!(error, DriveError::Discovery(_)), "{error}");
        }
    }
}
