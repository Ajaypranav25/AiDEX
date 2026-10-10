//! `.tar.gz` writing and extraction.

use std::{
    fs::File,
    io::{BufReader, BufWriter},
    path::Path,
};

use flate2::{Compression, GzBuilder, read::GzDecoder};
use tar::{Archive, Builder, EntryType, Header};

use super::{Extractor, Member, unsafe_entry};
use crate::BundleError;

pub(super) fn write(
    out: &Path,
    root: &str,
    members: &[Member],
    mtime: u64,
) -> Result<(), BundleError> {
    let error = |what: &str, error: std::io::Error| {
        BundleError::Archive(format!(
            "cannot write `{}` ({what}): {error}",
            out.display()
        ))
    };
    let file = File::create(out)
        .map_err(|e| BundleError::io(format!("cannot create `{}`", out.display()), e))?;
    // No file name and a zero timestamp in the gzip header; OS byte is "unknown".
    let encoder = GzBuilder::new()
        .mtime(0)
        .operating_system(255)
        .write(BufWriter::new(file), Compression::best());
    let mut builder = Builder::new(encoder);
    let header = |size: u64, mode: u32, kind: EntryType| {
        let mut header = Header::new_gnu();
        header.set_entry_type(kind);
        header.set_size(size);
        header.set_mode(mode);
        header.set_mtime(mtime);
        header.set_uid(0);
        header.set_gid(0);
        header
    };
    let mut directory = header(0, 0o755, EntryType::Directory);
    builder
        .append_data(&mut directory, format!("{root}/"), std::io::empty())
        .map_err(|e| error(root, e))?;
    for member in members {
        let input = File::open(&member.source).map_err(|e| {
            BundleError::io(format!("cannot read `{}`", member.source.display()), e)
        })?;
        let size = input
            .metadata()
            .map_err(|e| BundleError::io(format!("cannot read `{}`", member.source.display()), e))?
            .len();
        let mut entry = header(size, member.mode, EntryType::Regular);
        builder
            .append_data(&mut entry, &member.name, BufReader::new(input))
            .map_err(|e| error(&member.name, e))?;
    }
    let encoder = builder.into_inner().map_err(|e| error("tar trailer", e))?;
    let writer = encoder.finish().map_err(|e| error("gzip trailer", e))?;
    let file = writer
        .into_inner()
        .map_err(|e| error("flush", std::io::Error::other(e.error().to_string())))?;
    file.sync_all()
        .map_err(|e| BundleError::io(format!("cannot sync `{}`", out.display()), e))
}

pub(super) fn extract(archive: &Path, sink: &mut Extractor) -> Result<(), BundleError> {
    let file = File::open(archive)
        .map_err(|e| BundleError::io(format!("cannot open `{}`", archive.display()), e))?;
    let mut tar = Archive::new(GzDecoder::new(BufReader::new(file)));
    let entries = tar
        .entries()
        .map_err(|e| BundleError::Archive(format!("`{}`: {e}", archive.display())))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| BundleError::Archive(format!("corrupt entry: {e}")))?;
        let name = String::from_utf8(entry.path_bytes().into_owned())
            .map_err(|_| BundleError::Archive("an entry name is not valid UTF-8".into()))?;
        match entry.header().entry_type() {
            EntryType::Directory => sink.directory(&name)?,
            EntryType::Regular | EntryType::Continuous => {
                let size = entry.size();
                sink.file(&name, size, &mut entry)?;
            }
            other => {
                return Err(unsafe_entry(
                    &name,
                    format!(
                        "unsupported entry type {other:?}; links and special files are not allowed in bundles"
                    ),
                ));
            }
        }
    }
    Ok(())
}
