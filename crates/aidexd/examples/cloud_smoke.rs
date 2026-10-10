//! Inspect only safe connection metadata, or send one explicit cloud-chat smoke prompt.
use aidexd::Client;
use serde_json::json;
use std::{
    path::Path,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let drive = args.get(1).ok_or("usage: cloud_smoke <drive> [chat]")?;
    let mut client = Client::connect(Path::new(drive), "aidex-cloud-smoke")?;
    if args.get(2).map(String::as_str) == Some("workspace") {
        let path = args
            .get(3)
            .ok_or("workspace mode requires an existing repository folder")?;
        let list = client.call("workspace.list", json!({}))?;
        let existing = list["workspaces"]
            .as_array()
            .and_then(|entries| {
                entries.iter().find(|entry| {
                    entry["path"]
                        .as_str()
                        .is_some_and(|stored| stored.eq_ignore_ascii_case(path))
                })
            })
            .cloned();
        let workspace = if let Some(entry) = existing {
            entry
        } else {
            client.call("workspace.add", json!({"path":path,"name":"AiDEX"}))?
        };
        client.call("workspace.select", json!({"id_or_name":workspace["id"]}))?;
        let tree = client.call(
            "workspace.files.tree",
            json!({"workspace_id":workspace["id"],"recursive":false,"max_entries":50}),
        )?;
        let file = client.call(
            "workspace.files.read",
            json!({"workspace_id":workspace["id"],"path":"README.md"}),
        )?;
        let search=client.call("workspace.files.search",json!({"workspace_id":workspace["id"],"pattern":"AiDEX","glob":"README.md","max_results":10}))?;
        let git = client.call(
            "workspace.git.status",
            json!({"workspace_id":workspace["id"]}),
        )?;
        println!(
            "Workspace {} verified: {} root entries, README {} bytes, {} search hits, branch {}",
            workspace["name"],
            tree["entries"].as_array().map(Vec::len).unwrap_or(0),
            file["size"],
            search["matches"].as_array().map(Vec::len).unwrap_or(0),
            git["branch"]["head"]
        );
        return Ok(());
    }
    if args.get(2).map(String::as_str) == Some("local") {
        client.set_call_timeout(Duration::from_secs(180));
        let runtime = args
            .get(3)
            .ok_or("local requires a verified runtime directory and GGUF file")?;
        let model_file = args.get(4).ok_or("local requires a GGUF file")?;
        let catalog = client.call("inference.catalog", json!({}))?;
        println!("Checking verified llama.cpp runtime...");
        let installed = if catalog["runtime"]["verified"] == true {
            catalog["runtime"].clone()
        } else {
            client.call("inference.runtime.install", json!({"source":runtime}))?
        };
        println!("Runtime verified: {}", installed["release"]);
        println!("Importing GGUF model to the portable drive...");
        let existing = catalog["models"]
            .as_array()
            .and_then(|models| {
                models.iter().find(|model| {
                    model["name"] == "Qwen2.5 Coder 0.5B Instruct Q4_K_M"
                        && model["availability"] == "available"
                })
            })
            .cloned();
        let model = if let Some(model) = existing {
            model
        } else {
            client.call(
                "inference.import",
                json!({"source":model_file,"name":"Qwen2.5 Coder 0.5B Instruct Q4_K_M"}),
            )?
        };
        println!("Model imported: {} bytes", model["size_bytes"]);
        let status = client.call(
            "inference.start",
            json!({"model_id":model["id"],"backend":"cpu","context_window":2048,"threads":2}),
        )?;
        client.call(
            "model.use",
            json!({"provider_id":"local-llama","model_id":model["id"]}),
        )?;
        println!("{}", serde_json::to_string(&status)?);
        return Ok(());
    }
    if args.get(2).map(String::as_str) == Some("signin-new") {
        let mut bytes = [0u8; 4];
        getrandom::fill(&mut bytes).map_err(|_| "secure random source unavailable")?;
        let provider = format!("openai-chatgpt-{:08x}", u32::from_le_bytes(bytes));
        let flow = client.call(
            "auth.start",
            json!({"provider_id":provider,"method":"chatgpt"}),
        )?;
        client.call(
            "auth.open",
            json!({"authorization_url":flow["authorization_url"]}),
        )?;
        println!(
            "Fresh sign-in opened for account {provider}; choose your ChatGPT account and workspace."
        );
        return Ok(());
    }
    let provider = args.get(3).map(String::as_str).unwrap_or("openai");
    let status = client.call("auth.status", json!({"provider_id":provider}))?;
    println!("auth state: {}", status["state"]);
    if let Some(error) = status["error"].as_str() {
        println!("auth error: {error}");
    }
    let local_chat = args.get(2).map(String::as_str) == Some("local-chat");
    if args.get(2).map(String::as_str) != Some("chat") && !local_chat {
        return Ok(());
    }
    if !local_chat && status["state"] != "connected" {
        return Err("Complete ChatGPT sign-in in the desktop first".into());
    }
    let listing = client.call("model.list", json!({}))?;
    let target = &listing["default_model"];
    if target.is_null() {
        return Err("Select an available model in the desktop first".into());
    }
    if local_chat && target["provider_id"] != "local-llama" {
        return Err("Select the local model before a local chat check".into());
    }
    let session=client.call("conversation.create",json!({"title":if local_chat {"Local chat verification"} else {"Cloud chat verification"},"provider_id":target["provider_id"],"model_id":target["model_id"],"mode":"chat","workspace_id":null,"max_tokens":128,"max_steps":1}))?;
    if !session["workspace_id"].is_null() {
        return Err("Standalone chat incorrectly inherited a workspace".into());
    }
    let run=client.call("message.send",json!({"session_id":session["id"],"content":"Reply with one sentence confirming this cloud chat connection works.","command_id":format!("smoke-{}",session["id"].as_str().ok_or("missing id")?)}))?;
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let state = client.call("run.get", json!({"id":run["id"]}))?;
        match state["status"].as_str() {
            Some("completed") => {
                let history = client.call("conversation.get", json!({"id":session["id"]}))?;
                let response = history["messages"]
                    .as_array()
                    .ok_or("missing messages")?
                    .iter()
                    .rev()
                    .find(|m| m["role"] == "assistant")
                    .and_then(|m| m["content"].as_str())
                    .ok_or("missing assistant reply")?;
                if response.is_empty() {
                    return Err("empty assistant response".into());
                }
                println!(
                    "{} chat passed without a workspace: {response}",
                    if local_chat { "local" } else { "cloud" }
                );
                return Ok(());
            }
            Some("failed" | "cancelled" | "interrupted") => {
                return Err(format!("cloud chat failed: {}", state["error"]).into());
            }
            _ => {}
        }
        if Instant::now() >= deadline {
            let _ = client.call("run.cancel", json!({"id":run["id"]}));
            return Err("cloud chat timed out".into());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}
