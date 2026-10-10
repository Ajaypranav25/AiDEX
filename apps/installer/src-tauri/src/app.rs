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

    #[test]
    fn the_dev_url_is_the_address_the_next_dev_server_binds() {
        let config: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        // `bun run dev` binds Next.js to 127.0.0.1 only; `localhost` may resolve to ::1 first.
        assert_eq!(config["build"]["devUrl"], "http://127.0.0.1:3312");
    }

    #[test]
    fn dev_scripts_and_hot_reload_come_from_the_dev_url_address() {
        let config: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let dev_url = config["build"]["devUrl"].as_str().unwrap();
        let (host, port) = dev_url
            .trim_start_matches("http://")
            .split_once(':')
            .unwrap();
        // In development `assetPrefix` serves every script and the hot-reload socket.
        let next_config = include_str!("../../next.config.ts");
        assert!(
            next_config.contains(&format!("TAURI_DEV_HOST ?? \"{host}\"")),
            "next.config.ts must default devHost to {host}"
        );
        assert!(
            next_config.contains(&format!("`http://${{devHost}}:{port}`")),
            "next.config.ts must serve assets on port {port}"
        );
    }

    #[test]
    fn the_csp_allows_bundled_assets_and_ipc_only() {
        let config: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        // Without `ipc:` the first command's fetch is blocked and every command then falls back to
        // the slower postMessage transport. Tauri applies the CSP only to the assets it serves, never
        // to the dev server's pages, so neither tests nor `tauri dev` exercise it. Widening it is a
        // security change: do it here.
        assert_eq!(
            config["app"]["security"]["csp"],
            "default-src 'self'; connect-src ipc: http://ipc.localhost; style-src 'self' 'unsafe-inline'; img-src 'self' data:"
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
