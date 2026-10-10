// Sample setup catalog until drive discovery and the portable distribution manifest exist.
// Drives are examples, sizes are planning estimates, and nothing is read from hardware or written to a drive.

export type DriveId = "samsung" | "kingston" | "small";
export type Preset = "lite" | "developer" | "custom";
export type ModelId = "gemma" | "embedding" | "custom";
export type AgentId = "codex" | "claude" | "omp";
export type ExtensionId = "skills" | "mcp" | "toolchains";
export type Backend = "auto" | "cpu" | "gpu";
export type VaultMode = "password" | "none";

export type Drive = { id: DriveId; name: string; letter: string; totalGb: number; freeGb: number; kind: string };

export type SetupState = {
  driveId: DriveId;
  folder: string;
  preset: Preset;
  models: readonly ModelId[];
  customModelPath: string;
  backend: Backend;
  agents: readonly AgentId[];
  extensions: readonly ExtensionId[];
  vault: VaultMode;
};

export const drives: readonly Drive[] = [
  { id: "samsung", name: "Samsung T7 Portable SSD", letter: "E:", totalGb: 1000, freeGb: 632, kind: "External SSD" },
  { id: "kingston", name: "Kingston DataTraveler", letter: "F:", totalGb: 64, freeGb: 42.8, kind: "USB flash drive" },
  { id: "small", name: "Sample small USB", letter: "G:", totalGb: 8, freeGb: 2.3, kind: "USB flash drive" },
];

export const driveById = (id: DriveId): Drive => drives.find((drive) => drive.id === id) ?? drives[0];

/** AiDEX app, native agent harness, and the llama.cpp runtime are always included. */
const CORE_GB = 1.1;
const MODEL_GB: Record<ModelId, number> = { gemma: 6.2, embedding: 0.45, custom: 1.1 };
const AGENT_GB: Record<AgentId, number> = { codex: 0.3, claude: 0.3, omp: 0.3 };
const EXTENSION_GB: Record<ExtensionId, number> = { skills: 0.1, mcp: 0.2, toolchains: 0.8 };

const sumSizes = <Id extends string>(ids: readonly Id[], sizes: Record<Id, number>) =>
  ids.reduce((total, id) => total + sizes[id], 0);

export function estimateGb(state: SetupState): number {
  return CORE_GB + sumSizes(state.models, MODEL_GB) + sumSizes(state.agents, AGENT_GB) + sumSizes(state.extensions, EXTENSION_GB);
}

export const formatGb = (gb: number): string => `${gb.toFixed(1)} GB`;

export const fitsOnDrive = (state: SetupState): boolean => estimateGb(state) <= driveById(state.driveId).freeGb;

export const installPath = (state: SetupState): string => `${driveById(state.driveId).letter}\\${state.folder}`;

const PRESET_SELECTIONS: Record<Exclude<Preset, "custom">, Pick<SetupState, "models" | "agents" | "extensions">> = {
  lite: { models: [], agents: [], extensions: [] },
  developer: { models: ["gemma", "embedding"], agents: ["codex", "claude", "omp"], extensions: ["skills", "mcp", "toolchains"] },
};

/** Lite and Developer reset the component selections; Custom keeps the current ones so they can be refined. */
export function applyPreset(state: SetupState, preset: Preset): SetupState {
  return preset === "custom" ? { ...state, preset } : { ...state, preset, ...PRESET_SELECTIONS[preset] };
}

export const initialSetup: SetupState = {
  driveId: "samsung",
  folder: "AiDEX",
  preset: "developer",
  ...PRESET_SELECTIONS.developer,
  customModelPath: "",
  backend: "auto",
  // Durable private data is stored encrypted by default (docs/security.md).
  vault: "password",
};

export const presetOptions = [
  { value: "lite", title: "Lite", description: "AiDEX app and native agent harness. No bundled model or external tools.", meta: "Smallest" },
  { value: "developer", title: "Developer", description: "Recommended portable setup with local AI and CLI agents.", meta: "Recommended" },
  { value: "custom", title: "Custom", description: "Start with the developer setup, then choose exactly what to include.", meta: "Flexible" },
] as const;

export const modelOptions = [
  { value: "gemma", title: "Gemma 4 (GGUF)", description: "Portable sample model package for offline tasks.", meta: formatGb(MODEL_GB.gemma) },
  { value: "embedding", title: "Embedding model", description: "Optional local repository search and retrieval.", meta: formatGb(MODEL_GB.embedding) },
  { value: "custom", title: "Bring your own GGUF", description: "Add a model file you already have." },
] as const;

export const agentOptions = [
  { value: "codex", title: "codex", description: "Coding CLI adapter", meta: formatGb(AGENT_GB.codex) },
  { value: "claude", title: "claude code", description: "External coding agent CLI", meta: formatGb(AGENT_GB.claude) },
  { value: "omp", title: "omp", description: "Advanced agent harness", meta: formatGb(AGENT_GB.omp) },
] as const;

export const extensionOptions = [
  { value: "skills", title: "Skills", description: "Reusable agent workflows", meta: formatGb(EXTENSION_GB.skills) },
  { value: "mcp", title: "MCP servers", description: "Tools and connectors", meta: formatGb(EXTENSION_GB.mcp) },
  { value: "toolchains", title: "Toolchains", description: "Developer runtime helpers", meta: formatGb(EXTENSION_GB.toolchains) },
] as const;

export const backendOptions = [
  { value: "auto", label: "Auto detect" },
  { value: "cpu", label: "CPU only" },
  { value: "gpu", label: "Prefer GPU" },
] as const;
