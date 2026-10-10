//! `aidex runtime list|install|plan`: pinned llama.cpp runtimes on the drive and
//! the host-aware launch plan for a local model.

use crate::{
    commands::{
        daemon::connect,
        model::local::{LoadArgs, absolute_file, long_call, with_advice},
    },
    exit::{CliError, Exit},
    output::Output,
};
use aidex_inference::{LaunchPlan, PlanReport, RuntimeInventory, runtime::InstalledRuntime};
use aidexd::methods::inference::{RUNTIME_INSTALL, RUNTIME_LIST, RUNTIME_SELECT};
use clap::{Args, Subcommand};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct RuntimeArgs {
    /// Drive root; defaults to AIDEX_DRIVE_ROOT from the portable launcher.
    #[arg(long, value_name = "DRIVE_PATH", global = true)]
    pub drive: Option<PathBuf>,
    #[command(subcommand)]
    pub command: RuntimeCommand,
}

#[derive(Debug, Subcommand)]
pub enum RuntimeCommand {
    /// Show pinned runtimes for this platform and the runtimes installed on the drive.
    List,
    /// Install a pinned llama.cpp release archive (matched by SHA-256).
    Install {
        #[arg(value_name = "ARCHIVE")]
        archive: PathBuf,
    },
    /// Explain how a local model would be launched on this host.
    Plan {
        #[arg(value_name = "MODEL")]
        model: String,
        /// Context tokens to require (refused, never shrunk, if it cannot fit).
        #[arg(long, value_name = "TOKENS")]
        context: Option<u32>,
        /// Never offload to a GPU.
        #[arg(long)]
        cpu_only: bool,
        /// Read the model into RAM instead of memory-mapping it.
        #[arg(long)]
        no_mmap: bool,
        /// Run the opt-in GPU probe (`nvidia-smi`) first.
        #[arg(long)]
        probe_gpu: bool,
    },
}

impl RuntimeCommand {
    pub fn label(&self) -> &'static str {
        match self {
            Self::List => "aidex runtime list",
            Self::Install { .. } => "aidex runtime install",
            Self::Plan { .. } => "aidex runtime plan",
        }
    }
}

pub fn run(args: RuntimeArgs, output: Output) -> Result<Exit, CliError> {
    let mut client = connect(args.drive.as_deref())?;
    match args.command {
        RuntimeCommand::List => {
            let inventory: RuntimeInventory = client.call_as(RUNTIME_LIST, Value::Null)?;
            output.emit(&inventory, || print_inventory(&inventory))?;
        }
        RuntimeCommand::Install { archive } => {
            let archive = absolute_file(&archive, "runtime archive")?;
            let installed: InstalledRuntime =
                long_call(&mut client, RUNTIME_INSTALL, json!({ "archive": archive }))?;
            output.emit(&installed, || {
                let manifest = &installed.manifest;
                println!(
                    "Installed {} ({} {}, {} files) at {}",
                    manifest.catalog_id,
                    manifest.engine,
                    manifest.runtime_version,
                    manifest.files.len(),
                    installed.relative_dir
                );
                println!("  licence: {}", manifest.license);
                println!("  source:  {}", manifest.source);
                println!(
                    "  archive: {} ({})",
                    manifest.archive, manifest.archive_sha256
                );
            })?;
        }
        RuntimeCommand::Plan {
            model,
            context,
            cpu_only,
            no_mmap,
            probe_gpu,
        } => {
            let params = LoadArgs {
                id: model,
                context,
                cpu_only,
                no_mmap,
                probe_gpu,
            }
            .params();
            let report: PlanReport = client
                .call_as(RUNTIME_SELECT, params)
                .map_err(with_advice)?;
            output.emit(&report, || {
                let resources = &report.host.resources;
                println!(
                    "Host: {} {} ({}), {} physical / {} logical cores, {} of {} RAM available",
                    report.host.os,
                    report.host.arch,
                    report
                        .host
                        .platform
                        .as_deref()
                        .unwrap_or("unsupported platform"),
                    count(resources.physical_cores),
                    count(resources.logical_cores),
                    gib(resources.available_ram_bytes),
                    gib(resources.total_ram_bytes)
                );
                println!("GPU:  {}", report.host.gpu.summary());
                print_plan(&report.plan);
            })?;
        }
    }
    Ok(Exit::Success)
}

fn count(value: Option<usize>) -> String {
    value.map_or_else(|| "?".into(), |value| value.to_string())
}

fn gib(bytes: Option<u64>) -> String {
    bytes.map_or_else(
        || "unknown".into(),
        |bytes| format!("{:.1} GiB", bytes as f64 / (1u64 << 30) as f64),
    )
}

/// Print a launch plan with its reasons.
pub(crate) fn print_plan(plan: &LaunchPlan) {
    println!(
        "Plan: {} ({}), {} threads, context {} tokens, batch {}, {} GPU layers, mmap {}",
        plan.runtime.catalog_id,
        plan.runtime.variant,
        plan.threads,
        plan.ctx_size,
        plan.batch_size,
        plan.gpu_layers,
        if plan.mmap { "on" } else { "off" }
    );
    println!(
        "Estimated memory: {} MiB RAM{}",
        plan.estimated_ram_bytes >> 20,
        if plan.estimated_vram_bytes > 0 {
            format!(", {} MiB VRAM", plan.estimated_vram_bytes >> 20)
        } else {
            String::new()
        }
    );
    println!("Reasons:");
    for reason in &plan.reasons {
        println!("  - {reason}");
    }
}

fn print_inventory(inventory: &RuntimeInventory) {
    match &inventory.platform {
        Some(platform) => println!("Platform: {platform}"),
        None => println!("Platform: unsupported (no pinned runtimes)"),
    }
    if inventory.catalog.is_empty() {
        println!("Pinned runtimes: none for this platform");
    } else {
        println!("Pinned runtimes:");
        for entry in &inventory.catalog {
            println!(
                "  {} ({} {}, {}) {}{}",
                entry.archive,
                entry.engine,
                entry.version,
                entry.variant,
                if entry.validated {
                    "validated"
                } else {
                    "not validated"
                },
                entry
                    .notes
                    .as_deref()
                    .map(|notes| format!(" - {notes}"))
                    .unwrap_or_default()
            );
            println!("    sha256 {}  {}", entry.sha256, entry.url);
        }
    }
    if inventory.installed.runtimes.is_empty() {
        println!("Installed: none (install one with `aidex runtime install <ARCHIVE>`)");
    } else {
        println!("Installed:");
        for runtime in &inventory.installed.runtimes {
            println!(
                "  {} at {} (licence {})",
                runtime.manifest.catalog_id, runtime.relative_dir, runtime.manifest.license
            );
        }
    }
    for problem in &inventory.installed.problems {
        eprintln!("warning: {}: {}", problem.dir, problem.message);
    }
}
