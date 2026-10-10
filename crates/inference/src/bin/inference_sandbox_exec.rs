//! Test fixture: the `aidex-sandbox-exec` helper built from this package, so
//! integration tests can launch managed servers (`Sandbox::new(env!(..))`).

fn main() {
    std::process::exit(aidex_sandbox::helper::run(std::env::args_os()));
}
