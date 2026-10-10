//! Manual real-model acceptance: cargo run -p aidex-inference --example prove_local -- <distribution> <model.gguf>
use aidex_core::CancelToken;
use aidex_drive::{DriveLayout, DrivePaths};
use aidex_inference::{Backend, InferenceService, LOCAL_PROVIDER_ID, StartOptions};
use aidex_providers::{
    ChatMessage, ChatRequest, ModelId, NoVault, OfflineMode, ProviderId, ProviderRegistry,
};
use aidex_sandbox::Sandbox;
use std::{path::PathBuf, sync::Arc};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 3 {
        return Err("usage: prove_local <verified distribution directory> <GGUF file>".into());
    }
    let drive = tempfile::tempdir()?;
    let gate = Arc::new(OfflineMode::new(true));
    let registry = Arc::new(ProviderRegistry::in_memory(
        Arc::new(NoVault::default()),
        gate.clone(),
    ));
    let executable = std::env::current_exe()?;
    let helper = executable
        .parent()
        .and_then(|p| p.parent())
        .ok_or("missing target directory")?
        .join(format!(
            "aidex-sandbox-exec{}",
            std::env::consts::EXE_SUFFIX
        ));
    let service = InferenceService::open(
        DrivePaths::from_layout(drive.path().to_path_buf(), DriveLayout::default()),
        registry.clone(),
        gate,
        Sandbox::new(helper),
    )?;
    let runtime = service.install_runtime(Some(&PathBuf::from(&args[1])), &CancelToken::new())?;
    let model = service.import(
        &PathBuf::from(&args[2]),
        Some("Qwen tiny acceptance"),
        &CancelToken::new(),
    )?;
    let status = service.start(
        StartOptions {
            model_id: model.id.clone(),
            backend: Backend::Auto,
            context_window: Some(2048),
            threads: Some(2),
        },
        &CancelToken::new(),
    )?;
    let provider = registry
        .provider(&ProviderId::new(LOCAL_PROVIDER_ID)?)
        .ok_or("provider not registered")?;
    let request = ChatRequest::new(
        ModelId::new(model.id.clone())?,
        vec![ChatMessage::user(
            "Reply with exactly: AiDEX local inference works",
        )],
    )
    .with_max_tokens(32);
    let response = provider.chat(&request, &mut |_| {}, &CancelToken::new())?;
    let stopped = service.stop()?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"runtime":runtime,"model":model,"status":status,"response":response,"stopped":stopped,"registered_after_stop":registry.providers().len(),"offline":true})
        )?
    );
    Ok(())
}
