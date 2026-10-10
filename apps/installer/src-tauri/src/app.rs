//! Tauri composition for the setup wizard.

use crate::commands;

/// Build and run the setup app.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::list_drives,
            commands::inspect_drive,
            commands::init_drive,
            commands::host_profile,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run AiDEX setup");
}

#[cfg(test)]
mod tests {
    use crate::commands::COMMANDS;
    use serde_json::Value;

    #[test]
    fn the_capability_grants_exactly_the_registered_commands() {
        let capability: Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        let mut granted: Vec<String> = capability["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|permission| permission.as_str().unwrap().to_owned())
            .collect();
        granted.sort();
        let mut expected: Vec<String> = COMMANDS
            .iter()
            .map(|command| format!("allow-{}", command.replace('_', "-")))
            .collect();
        expected.sort();
        assert_eq!(granted, expected);
    }

    /// The sources a CSP grants for one directive, e.g. `connect-src`.
    fn csp_sources<'a>(csp: &'a str, directive: &str) -> Vec<&'a str> {
        csp.split(';')
            .map(|entry| entry.split_whitespace().collect::<Vec<_>>())
            .find(|words| words.first() == Some(&directive))
            .map(|words| words[1..].to_vec())
            .unwrap_or_default()
    }

    #[test]
    fn the_dev_url_is_the_address_the_next_dev_server_binds() {
        let config: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        // `bun run dev` binds Next.js to 127.0.0.1 only; `localhost` may resolve to ::1 first.
        assert_eq!(config["build"]["devUrl"], "http://127.0.0.1:3312");
    }

    #[test]
    fn the_csp_lets_the_page_reach_ipc_and_nothing_else() {
        let config: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let csp = config["app"]["security"]["csp"].as_str().unwrap();
        // Without `ipc:` every command first fails a blocked fetch, then falls back to the
        // slower postMessage transport. Tauri never applies the CSP to the dev server's pages,
        // so the dev server's hot-reload socket does not belong in it.
        assert_eq!(
            csp_sources(csp, "connect-src"),
            ["ipc:", "http://ipc.localhost"]
        );
    }

    #[test]
    fn the_build_manifest_and_handler_list_name_every_command() {
        let build = include_str!("../build.rs");
        let app = include_str!("app.rs");
        for command in COMMANDS {
            assert!(
                build.contains(&format!("\"{command}\"")),
                "build.rs lacks {command}"
            );
            assert!(
                app.contains(&format!("commands::{command},")),
                "app.rs lacks {command}"
            );
        }
    }
}
