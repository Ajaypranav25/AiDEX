//! `aidex model list|use|route|default`: inspect models (context limits, tool
//! support, locality), choose the default, and explain routing decisions.

use crate::{
    commands::daemon::connect,
    exit::{CliError, Exit},
    output::Output,
};
use aidex_providers::{ModelDescriptor, ModelRef, ProviderFailure};
use aidex_router::{PrivacyMode, Rejection, TaskKind};
use aidexd::{
    ClientError,
    methods::models::{
        DefaultModel, MODEL_DEFAULT, MODEL_LIST, MODEL_ROUTE, MODEL_USE, ModelList, RouteResult,
    },
};
use clap::{Args, Subcommand};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct ModelArgs {
    /// Drive root; defaults to AIDEX_DRIVE_ROOT from the portable launcher.
    #[arg(long, value_name = "DRIVE_PATH", global = true)]
    pub drive: Option<PathBuf>,
    #[command(subcommand)]
    pub command: ModelCommand,
}

#[derive(Debug, Subcommand)]
pub enum ModelCommand {
    /// List models with context limits, tool support, and locality.
    List,
    /// Set the default model, written PROVIDER/MODEL (for example local/qwen).
    Use {
        #[arg(value_name = "PROVIDER/MODEL")]
        model: String,
    },
    /// Explain which model a task would use, with fallbacks and rejections.
    Route {
        /// plan, code, edit, summarize, classify, embed, verify, or chat.
        #[arg(long)]
        task: TaskKind,
        /// local-only, hybrid (default), cloud-preferred, or air-gapped.
        #[arg(long)]
        privacy: Option<PrivacyMode>,
        /// Context tokens the task needs.
        #[arg(long, value_name = "TOKENS")]
        context: Option<u32>,
        /// Require tool calling.
        #[arg(long)]
        tools: bool,
        /// Require JSON-schema output.
        #[arg(long)]
        json_schema: bool,
        /// The task handles secrets (local models only).
        #[arg(long)]
        secret_heavy: bool,
        /// Explicit PROVIDER/MODEL override (refused, never swapped, if unusable).
        #[arg(long, value_name = "PROVIDER/MODEL")]
        model: Option<String>,
    },
    /// Show the default model.
    Default,
}

impl ModelCommand {
    pub fn label(&self) -> &'static str {
        match self {
            Self::List => "aidex model list",
            Self::Use { .. } => "aidex model use",
            Self::Route { .. } => "aidex model route",
            Self::Default => "aidex model default",
        }
    }
}

pub fn run(args: ModelArgs, output: Output) -> Result<Exit, CliError> {
    let mut client = connect(args.drive.as_deref())?;
    match args.command {
        ModelCommand::List => {
            let list: ModelList = client.call_as(MODEL_LIST, Value::Null)?;
            output.emit(&list, || print_list(&list))?;
        }
        ModelCommand::Use { model } => {
            let model = ModelRef::parse(&model)
                .map_err(|error| CliError::new(Exit::Usage, error.to_string()))?;
            let result: DefaultModel = client.call_as(
                MODEL_USE,
                json!({ "provider_id": model.provider_id, "model_id": model.model_id }),
            )?;
            output.emit(&result, || {
                println!("Default model: {model}");
                if let Some(descriptor) = &result.model {
                    println!("{}", describe(descriptor));
                }
            })?;
        }
        ModelCommand::Route {
            task,
            privacy,
            context,
            tools,
            json_schema,
            secret_heavy,
            model,
        } => {
            let params = json!({
                "task": task,
                "privacy": privacy,
                "required_context": context.unwrap_or(0),
                "needs_tools": tools,
                "needs_json_schema": json_schema,
                "secret_heavy": secret_heavy,
                "model": model,
            });
            let routed: RouteResult = match client.call_as(MODEL_ROUTE, params) {
                Ok(routed) => routed,
                Err(error) => {
                    if !output.is_json() {
                        print_route_failure(&error);
                    }
                    return Err(CliError::from(error).with_hint(
                        "inspect models with `aidex model list`; relax --context/--tools/--privacy or go online with `aidex offline off`",
                    ));
                }
            };
            output.emit(&routed, || print_route(&routed))?;
        }
        ModelCommand::Default => {
            let result: DefaultModel = client.call_as(MODEL_DEFAULT, Value::Null)?;
            output.emit(&result, || match &result.default_model {
                Some(model) => println!("Default model: {model}"),
                None => {
                    println!("Default model: none (set one with `aidex model use PROVIDER/MODEL`)")
                }
            })?;
        }
    }
    Ok(Exit::Success)
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

/// One-line capability summary of a model.
pub(crate) fn describe(model: &ModelDescriptor) -> String {
    let mut text = format!(
        "{} model, context {} tokens",
        model.locality.as_str(),
        model.context_window
    );
    if let Some(max) = model.max_output_tokens {
        text.push_str(&format!(" (max output {max})"));
    }
    text.push_str(&format!(
        ", tools {}, JSON schema {}, limits {}",
        model.capabilities.tool_calls.as_str(),
        yes_no(model.capabilities.json_schema_output),
        model.source.as_str()
    ));
    text
}

pub(crate) fn print_failures(failures: &[ProviderFailure]) {
    for failure in failures {
        eprintln!(
            "warning: provider {} ({}): {}",
            failure.provider_id,
            failure.kind.as_str(),
            failure.message
        );
    }
}

fn print_list(list: &ModelList) {
    if list.models.is_empty() {
        println!("No models available.");
    } else {
        let width = list
            .models
            .iter()
            .map(|model| model.model.to_string().len())
            .max()
            .unwrap_or(5)
            .max(5);
        println!(
            "  {:<width$}  {:<8}  {:>8}  {:<10}  {:<4}  {:<10}  ROLES",
            "MODEL", "LOCALITY", "CONTEXT", "TOOLS", "JSON", "LIMITS"
        );
        for model in &list.models {
            let marker = if list.default_model.as_ref() == Some(&model.model) {
                '*'
            } else {
                ' '
            };
            let roles: Vec<&str> = model.roles.iter().map(|role| role.as_str()).collect();
            println!(
                "{marker} {:<width$}  {:<8}  {:>8}  {:<10}  {:<4}  {:<10}  {}",
                model.model.to_string(),
                model.locality.as_str(),
                model.context_window,
                model.capabilities.tool_calls.as_str(),
                yes_no(model.capabilities.json_schema_output),
                model.source.as_str(),
                roles.join(",")
            );
        }
    }
    match &list.default_model {
        Some(model) => println!("Default model: {model}"),
        None => println!("Default model: none"),
    }
    println!("Network: {}", list.network);
    print_failures(&list.failures);
}

fn print_rejections(rejected: &[Rejection]) {
    if !rejected.is_empty() {
        println!("Rejected:");
        for rejection in rejected {
            println!("  - {}: {}", rejection.model, rejection.explanation);
        }
    }
}

fn print_route(routed: &RouteResult) {
    let decision = &routed.decision;
    println!(
        "Task: {} (privacy {}, network {})",
        decision.task, decision.privacy, routed.network
    );
    println!("Primary: {}", decision.primary);
    println!("  {}", describe(&routed.model));
    if decision.fallbacks.is_empty() {
        println!("Fallbacks: none");
    } else {
        let names: Vec<String> = decision.fallbacks.iter().map(ToString::to_string).collect();
        println!("Fallbacks: {}", names.join(" -> "));
    }
    println!("Reasons:");
    for reason in &decision.reasons {
        println!("  - {reason}");
    }
    print_rejections(&decision.rejected);
    print_failures(&routed.failures);
}

fn print_route_failure(error: &ClientError) {
    let ClientError::Rpc {
        data: Some(data), ..
    } = error
    else {
        return;
    };
    for reason in data["reasons"].as_array().into_iter().flatten() {
        if let Some(reason) = reason.as_str() {
            eprintln!("reason: {reason}");
        }
    }
    let rejected: Vec<Rejection> =
        serde_json::from_value(data["rejected"].clone()).unwrap_or_default();
    for rejection in rejected {
        eprintln!("rejected: {}: {}", rejection.model, rejection.explanation);
    }
    let failures: Vec<ProviderFailure> =
        serde_json::from_value(data["failures"].clone()).unwrap_or_default();
    print_failures(&failures);
}
