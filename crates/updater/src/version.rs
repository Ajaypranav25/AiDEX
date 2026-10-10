//! Which version changes an update may make.
//!
//! Versions are ordered by SemVer 2.0 precedence (build metadata ignored). A package
//! older than the active bundle is a downgrade and is refused, unless the caller
//! passes `allow_downgrade` **and** the package is exactly the recorded previous
//! bundle (same version and `bundle.json` digest) — that is, a rollback delivered as
//! a package. The same version is accepted only to repair a damaged installation.

use aidex_bundle::TargetPointer;
use serde::{Deserialize, Serialize};

use crate::UpdateError;

/// Options for [`crate::Updater::apply`] and [`crate::Updater::check`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyOptions {
    /// Permit returning to the recorded previous bundle with an older package.
    #[serde(default)]
    pub allow_downgrade: bool,
    /// Development only: accept a package without `bundle.sig` (a warning event is
    /// emitted). A present but invalid or untrusted signature is always refused.
    #[serde(default)]
    pub allow_unsigned: bool,
}

/// What applying a package does.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateAction {
    /// Nothing is installed for the target yet.
    Install,
    /// The package is newer than the active bundle.
    Upgrade,
    /// Same version, but the active bundle failed verification: reinstall it.
    Repair,
    /// Return to the recorded previous bundle (`allow_downgrade`).
    DowngradeToPrevious,
    /// The package is the active bundle and it verifies: nothing to do.
    UpToDate,
}

pub(crate) fn parse_semver(version: &str) -> Result<semver::Version, UpdateError> {
    semver::Version::parse(version).map_err(|_| UpdateError::InvalidVersion {
        version: version.to_owned(),
    })
}

/// Decide what applying `candidate` (version and `bundle.json` digest) means given
/// the active pointer entry. `current_intact` is consulted only for equal versions.
pub(crate) fn decide(
    current: Option<&TargetPointer>,
    candidate_version: &str,
    candidate_sha256: &str,
    options: &ApplyOptions,
    current_intact: &mut dyn FnMut() -> bool,
) -> Result<UpdateAction, UpdateError> {
    let candidate = parse_semver(candidate_version)?;
    let Some(current) = current else {
        return Ok(UpdateAction::Install);
    };
    let active = parse_semver(&current.version)?;
    match candidate.cmp_precedence(&active) {
        std::cmp::Ordering::Greater => Ok(UpdateAction::Upgrade),
        std::cmp::Ordering::Equal => {
            if current_intact() {
                if candidate_sha256 == current.manifest_sha256 {
                    Ok(UpdateAction::UpToDate)
                } else {
                    Err(UpdateError::NotNewer {
                        current: current.version.clone(),
                        candidate: candidate_version.to_owned(),
                    })
                }
            } else {
                Ok(UpdateAction::Repair)
            }
        }
        std::cmp::Ordering::Less => {
            if !options.allow_downgrade {
                return Err(UpdateError::Downgrade {
                    current: current.version.clone(),
                    candidate: candidate_version.to_owned(),
                });
            }
            match &current.previous {
                Some(previous)
                    if previous.version == candidate_version
                        && previous.manifest_sha256 == candidate_sha256 =>
                {
                    Ok(UpdateAction::DowngradeToPrevious)
                }
                previous => Err(UpdateError::DowngradeNotPrevious {
                    candidate: candidate_version.to_owned(),
                    previous: previous.as_ref().map(|previous| previous.version.clone()),
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aidex_bundle::VersionRef;

    fn entry(version: &str, sha: char, previous: Option<(&str, char)>) -> TargetPointer {
        TargetPointer {
            version: version.into(),
            dir: format!("versions/{version}-linux-x64"),
            manifest_sha256: sha.to_string().repeat(64),
            previous: previous.map(|(version, sha)| VersionRef {
                version: version.into(),
                dir: format!("versions/{version}-linux-x64"),
                manifest_sha256: sha.to_string().repeat(64),
            }),
        }
    }

    fn run(
        current: Option<&TargetPointer>,
        version: &str,
        sha: char,
        options: ApplyOptions,
        intact: bool,
    ) -> Result<UpdateAction, UpdateError> {
        decide(
            current,
            version,
            &sha.to_string().repeat(64),
            &options,
            &mut || intact,
        )
    }

    #[test]
    fn versions_order_by_semver_precedence() {
        let current = entry("0.2.0", 'a', Some(("0.1.0", 'b')));
        let plain = ApplyOptions::default();
        assert_eq!(
            run(None, "0.1.0", 'c', plain, true).unwrap(),
            UpdateAction::Install
        );
        for newer in ["0.2.1", "0.10.0", "1.0.0-rc.1"] {
            assert_eq!(
                run(Some(&current), newer, 'c', plain, true).unwrap(),
                UpdateAction::Upgrade,
                "{newer}"
            );
        }
        // Pre-releases sort before their release; build metadata does not order.
        assert!(matches!(
            run(Some(&current), "0.2.0-rc.9", 'c', plain, true),
            Err(UpdateError::Downgrade { .. })
        ));
        assert!(matches!(
            run(Some(&current), "0.2.0+build.7", 'c', plain, true),
            Err(UpdateError::NotNewer { .. })
        ));
        assert_eq!(
            run(Some(&current), "0.2.0", 'a', plain, true).unwrap(),
            UpdateAction::UpToDate
        );
        assert_eq!(
            run(Some(&current), "0.2.0", 'a', plain, false).unwrap(),
            UpdateAction::Repair
        );
        assert!(matches!(
            run(Some(&current), "2026.1", 'c', plain, true),
            Err(UpdateError::InvalidVersion { .. })
        ));
    }

    #[test]
    fn downgrades_need_the_flag_and_the_recorded_previous_bundle() {
        let current = entry("0.2.0", 'a', Some(("0.1.0", 'b')));
        let allow = ApplyOptions {
            allow_downgrade: true,
            ..ApplyOptions::default()
        };
        assert!(matches!(
            run(Some(&current), "0.1.0", 'b', ApplyOptions::default(), true),
            Err(UpdateError::Downgrade { .. })
        ));
        assert_eq!(
            run(Some(&current), "0.1.0", 'b', allow, true).unwrap(),
            UpdateAction::DowngradeToPrevious
        );
        // Same version as previous but a different build, or an older version.
        assert!(matches!(
            run(Some(&current), "0.1.0", 'c', allow, true),
            Err(UpdateError::DowngradeNotPrevious { .. })
        ));
        assert!(matches!(
            run(Some(&current), "0.0.9", 'b', allow, true),
            Err(UpdateError::DowngradeNotPrevious { .. })
        ));
        let lonely = entry("0.2.0", 'a', None);
        assert!(matches!(
            run(Some(&lonely), "0.1.0", 'b', allow, true),
            Err(UpdateError::DowngradeNotPrevious { previous: None, .. })
        ));
    }
}
