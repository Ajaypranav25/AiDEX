//! `aidex-packager install`: put a bundle onto an initialized drive.

use aidex_bundle::{InstallReport, InstallSource, install};
use aidex_drive::{InitOptions, init_with};

use crate::{cli::InstallArgs, error::PackagerError};

pub fn run(args: &InstallArgs) -> Result<InstallReport, PackagerError> {
    if args.init_dev_drive {
        init_with(
            &args.drive,
            InitOptions {
                require_removable: false,
            },
        )?;
    }
    let source = InstallSource::from_path(&args.bundle)?;
    Ok(install(&source, &args.drive)?)
}
