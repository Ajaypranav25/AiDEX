//! `aidex-packager install`: put a bundle onto an initialized drive.

use aidex_bundle::{InstallReport, InstallSource, install_signed};
use aidex_drive::{InitOptions, init_with};

use crate::{cli::InstallArgs, error::PackagerError, signing::policy};

/// Install after checking `bundle.sig` against the built-in keys plus `--trust-key`
/// files (unsigned bundles only with `--allow-unsigned`).
pub fn run(args: &InstallArgs) -> Result<InstallReport, PackagerError> {
    let policy = policy(&args.trust.trust_key, args.trust.allow_unsigned)?;
    if args.init_dev_drive {
        init_with(
            &args.drive,
            InitOptions {
                require_removable: false,
            },
        )?;
    }
    let source = InstallSource::from_path(&args.bundle)?;
    Ok(install_signed(&source, &args.drive, &policy, &mut |_| {
        Ok(())
    })?)
}
