//! `SHA256SUMS` files in the `sha256sum` text format (`<hex>  <name>`), verifiable with
//! `sha256sum -c SHA256SUMS` or `shasum -a 256 -c SHA256SUMS`.

use std::{collections::BTreeMap, fs, io::ErrorKind, path::Path};

use crate::{BundleError, digest::is_sha256_hex, fsutil::write_atomic};

/// Conventional file name.
pub const SHA256SUMS_FILE_NAME: &str = "SHA256SUMS";

/// Parsed checksum lines, keyed by file name.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Sha256Sums {
    entries: BTreeMap<String, String>,
}

impl Sha256Sums {
    /// Parse `sha256sum` output (text or binary `*` marker). Blank lines are ignored.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut entries = BTreeMap::new();
        for (number, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let (digest, rest) = line
                .split_once(' ')
                .ok_or_else(|| format!("line {}: expected `<sha256>  <name>`", number + 1))?;
            let name = rest
                .strip_prefix(' ')
                .or_else(|| rest.strip_prefix('*'))
                .ok_or_else(|| {
                    format!("line {}: expected two spaces after the digest", number + 1)
                })?;
            if !is_sha256_hex(digest) || name.is_empty() || name.contains(['/', '\\']) {
                return Err(format!("line {}: invalid entry", number + 1));
            }
            if entries.insert(name.to_owned(), digest.to_owned()).is_some() {
                return Err(format!("line {}: `{name}` is listed twice", number + 1));
            }
        }
        Ok(Self { entries })
    }

    /// Read `path`; a missing file is an empty list.
    pub fn load(path: &Path) -> Result<Self, BundleError> {
        match fs::read_to_string(path) {
            Ok(text) => Self::parse(&text).map_err(|reason| {
                BundleError::InvalidSource(format!("`{}`: {reason}", path.display()))
            }),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(BundleError::io(
                format!("cannot read `{}`", path.display()),
                error,
            )),
        }
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries.get(name).map(String::as_str)
    }

    /// Add or replace the digest of `name` (a bare file name).
    pub fn set(&mut self, name: &str, sha256: &str) -> Result<(), String> {
        if !is_sha256_hex(sha256) || name.is_empty() || name.contains(['/', '\\', '\n']) {
            return Err(format!("invalid checksum entry for `{name}`"));
        }
        self.entries.insert(name.to_owned(), sha256.to_owned());
        Ok(())
    }

    /// Lines sorted by file name.
    pub fn render(&self) -> String {
        self.entries
            .iter()
            .map(|(name, digest)| format!("{digest}  {name}\n"))
            .collect()
    }

    /// Atomically write the file.
    pub fn store(&self, path: &Path) -> Result<(), BundleError> {
        write_atomic(path, self.render().as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::digest::sha256_hex;

    #[test]
    fn round_trips_sorted_entries() {
        let mut sums = Sha256Sums::default();
        sums.set("b.tar.gz", &sha256_hex(b"b")).unwrap();
        sums.set("a.zip", &sha256_hex(b"a")).unwrap();
        sums.set("b.tar.gz", &sha256_hex(b"c")).unwrap();
        let text = sums.render();
        assert!(text.starts_with(&format!("{}  a.zip\n", sha256_hex(b"a"))));
        assert_eq!(Sha256Sums::parse(&text).unwrap(), sums);
        assert_eq!(sums.get("b.tar.gz"), Some(sha256_hex(b"c").as_str()));
        let binary = format!("{} *a.zip\n", sha256_hex(b"a"));
        assert_eq!(
            Sha256Sums::parse(&binary).unwrap().get("a.zip"),
            sums.get("a.zip")
        );
        assert!(Sha256Sums::parse("abc  a.zip").is_err());
        assert!(sums.set("../x", &sha256_hex(b"x")).is_err());
    }
}
