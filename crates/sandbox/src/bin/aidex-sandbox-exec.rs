//! `aidex-sandbox-exec`: applies an AiDEX sandbox plan to itself, then starts the target.
//! Started only by `aidex_sandbox::Sandbox`; it must ship next to `aidexd`.

fn main() {
    std::process::exit(aidex_sandbox::helper::run(std::env::args_os()));
}
