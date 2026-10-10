import type { PreviewTask } from "../../lib/usePreviewRun";

// Stages the real installer will run. In this preview they only advance on a timer.
export const previewTasks: readonly PreviewTask[] = [
  { title: "Create drive layout", plan: "Folders for the runtime, models, agents, workspaces, and vault." },
  { title: "Copy runtime", plan: "AiDEX core, llama.cpp, and the chosen acceleration backends." },
  { title: "Copy models", plan: "Selected GGUF models and the search embedding model." },
  { title: "Install agents", plan: "AiDEX Native, adapters, skill packs, MCP servers, and toolchains." },
  { title: "Create encrypted vault", plan: "Vault sealed with your password and recovery key." },
  { title: "Verify integrity", plan: "Every file checked against signed manifests." },
];

export const integrityChecks: readonly { id: string; check: string; scope: string }[] = [
  { id: "layout", check: "Drive layout", scope: "Folder structure and permissions" },
  { id: "runtime", check: "Runtime manifest", scope: "AiDEX core, llama.cpp, and backends" },
  { id: "models", check: "Model checksums", scope: "SHA-256 of every model file" },
  { id: "agents", check: "Agent adapters", scope: "Adapter, skill, and server manifests" },
  { id: "vault", check: "Vault header", scope: "Encryption parameters and recovery slot" },
];
