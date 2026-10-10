//! `.zip` writing and extraction (Deflate only).

use std::{
    fs::File,
    io::{self, BufReader, BufWriter},
    path::Path,
};

use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter, write::SimpleFileOptions};

use super::{Extractor, Member, unsafe_entry};
use crate::BundleError;

const SECONDS_PER_DAY: u64 = 86_400;
/// 1980-01-01T00:00:00Z, the earliest time a zip entry can record.
const ZIP_EPOCH: u64 = 315_532_800;

pub(super) fn write(
    out: &Path,
    root: &str,
    members: &[Member],
    mtime: u64,
) -> Result<(), BundleError> {
    let error = |what: &str, error: &dyn std::fmt::Display| {
        BundleError::Archive(format!(
            "cannot write `{}` ({what}): {error}",
            out.display()
        ))
    };
    let file = File::create(out)
        .map_err(|e| BundleError::io(format!("cannot create `{}`", out.display()), e))?;
    let mut writer = ZipWriter::new(BufWriter::new(file));
    let time = dos_time(mtime);
    let options = |mode: u32| {
        SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(9))
            .last_modified_time(time)
            .unix_permissions(mode)
    };
    writer
        .add_directory(format!("{root}/"), options(0o755))
        .map_err(|e| error(root, &e))?;
    for member in members {
        writer
            .start_file(member.name.as_str(), options(member.mode))
            .map_err(|e| error(&member.name, &e))?;
        let mut input = File::open(&member.source).map_err(|e| {
            BundleError::io(format!("cannot read `{}`", member.source.display()), e)
        })?;
        io::copy(&mut input, &mut writer).map_err(|e| error(&member.name, &e))?;
    }
    let inner = writer
        .finish()
        .map_err(|e| error("central directory", &e))?;
    let file = inner
        .into_inner()
        .map_err(|e| error("flush", &e.error().to_string()))?;
    file.sync_all()
        .map_err(|e| BundleError::io(format!("cannot sync `{}`", out.display()), e))
}

pub(super) fn extract(archive: &Path, sink: &mut Extractor) -> Result<(), BundleError> {
    let file = File::open(archive)
        .map_err(|e| BundleError::io(format!("cannot open `{}`", archive.display()), e))?;
    let mut zip = ZipArchive::new(BufReader::new(file))
        .map_err(|e| BundleError::Archive(format!("`{}`: {e}", archive.display())))?;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| BundleError::Archive(format!("entry {index}: {e}")))?;
        let name = entry.name().to_owned();
        if entry.is_symlink() {
            return Err(unsafe_entry(
                &name,
                "is a symbolic link; links are not allowed in bundles",
            ));
        }
        if entry.is_dir() {
            sink.directory(&name)?;
        } else if entry.is_file() {
            let size = entry.size();
            sink.file(&name, size, &mut entry)?;
        } else {
            return Err(unsafe_entry(&name, "is not a regular file or directory"));
        }
    }
    Ok(())
}

/// Convert Unix seconds to a zip (DOS) timestamp, clamped to the representable range.
fn dos_time(seconds: u64) -> DateTime {
    let seconds = seconds.max(ZIP_EPOCH);
    let days = i64::try_from(seconds / SECONDS_PER_DAY).unwrap_or(0);
    let within = seconds % SECONDS_PER_DAY;
    let (year, month, day) = civil_from_days(days);
    let clamp = |value: u64| u8::try_from(value).unwrap_or(0);
    DateTime::from_date_and_time(
        u16::try_from(year).unwrap_or(2107).min(2107),
        clamp(month),
        clamp(day),
        clamp(within / 3600),
        clamp(within / 60 % 60),
        clamp(within % 60 / 2 * 2),
    )
    .unwrap_or_default()
}

/// Days since 1970-01-01 to a proleptic Gregorian (year, month, day).
fn civil_from_days(days: i64) -> (i64, u64, u64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (
        year,
        u64::try_from(month).unwrap_or(1),
        u64::try_from(day).unwrap_or(1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates_match_known_days() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn dos_time_clamps_to_the_zip_epoch() {
        assert_eq!(dos_time(0), dos_time(ZIP_EPOCH));
        let time = dos_time(1_704_067_201); // 2024-01-01T00:00:01Z
        assert_eq!((time.year(), time.month(), time.day()), (2024, 1, 1));
        assert_eq!(time.second(), 0);
    }
}
