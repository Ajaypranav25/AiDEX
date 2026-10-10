// Sample setup catalog until drive discovery and the portable distribution manifest exist.
// Sizes are planning estimates; nothing here is read from hardware or written to a drive.

export type ComponentGroupId = "models" | "backends" | "agents" | "skills" | "mcp" | "toolchains";

export type SetupComponent = {
  id: string;
  name: string;
  sizeGb: number;
  description: string;
  /** Always installed; shown checked and disabled. */
  required?: boolean;
  recommended?: boolean;
};

export type Drive = {
  id: string;
  name: string;
  capacityGb: number;
  freeGb: number;
  connection: string;
  /** Why the drive cannot be used; the drive is shown disabled with this reason. */
  unavailableReason?: string;
};

export type ProfileId = "lite" | "developer" | "custom";

export type SetupSelections = {
  driveId: string | null;
  profile: ProfileId;
  components: Readonly<Record<ComponentGroupId, readonly string[]>>;
  ownerName: string;
  recoveryKeySaved: boolean;
};

/** AiDEX core, desktop app, and the llama.cpp inference runtime are always installed. */
export const CORE_SIZE_GB = 1.6;
export const RUNTIME = { name: "llama.cpp", sizeGb: 0.3 } as const;

export const sampleDrives: readonly Drive[] = [
  { id: "samsung-t7", name: "Samsung T7 Portable SSD", capacityGb: 1000, freeGb: 812, connection: "USB 3.2 Gen 2 · exFAT" },
  { id: "sandisk-extreme", name: "SanDisk Extreme Pro", capacityGb: 256, freeGb: 201, connection: "USB 3.2 Gen 2 · exFAT" },
  {
    id: "kingston-dt",
    name: "Kingston DataTraveler",
    capacityGb: 64,
    freeGb: 58,
    connection: "USB 3.0 · FAT32",
    unavailableReason: "Too small: AiDEX needs a drive of at least 128 GB.",
  },
];

export const catalog: Readonly<Record<ComponentGroupId, readonly SetupComponent[]>> = {
  models: [
    { id: "gemma-4-e4b", name: "Gemma 4 E4B Q4_K_M", sizeGb: 3.2, recommended: true, description: "Fast general chat and coding on most laptops." },
    { id: "qwen-coder-7b", name: "Qwen2.5 Coder 7B", sizeGb: 4.7, description: "Stronger code completion and refactoring." },
    { id: "llama-3b", name: "Llama 3.2 3B", sizeGb: 2.0, description: "Small model for low-memory hosts." },
    { id: "nomic-embed", name: "nomic-embed-text", sizeGb: 0.3, required: true, description: "Required for workspace search and memory." },
  ],
  backends: [
    { id: "cuda", name: "CUDA", sizeGb: 2.8, description: "NVIDIA GPUs. Sample host: GeForce RTX 4070 Laptop GPU detected." },
    { id: "vulkan", name: "Vulkan", sizeGb: 0.2, description: "Most AMD, Intel, and NVIDIA GPUs. Sample host: supported." },
    { id: "metal", name: "Metal", sizeGb: 0.1, description: "Apple silicon Macs. Not available on the sample host; include it to use the drive on a Mac." },
    { id: "cpu", name: "CPU fallback", sizeGb: 0.1, required: true, description: "Runs on any host when no GPU backend is available." },
  ],
  agents: [
    { id: "aidex-native", name: "AiDEX Native", sizeGb: 0.2, required: true, description: "Built-in agent runtime with approvals and checkpoints." },
    { id: "codex-cli", name: "Codex CLI", sizeGb: 0.2, description: "Optional adapter; you sign in with your own account." },
    { id: "claude-code", name: "Claude Code", sizeGb: 0.3, description: "Optional adapter; you sign in with your own account." },
    { id: "omp", name: "Oh My Pi (OMP)", sizeGb: 0.3, description: "Optional adapter for the Oh My Pi agent runtime." },
  ],
  skills: [
    { id: "skills-coding", name: "Coding essentials", sizeGb: 0.1, recommended: true, description: "Review, testing, and refactoring skills." },
    { id: "skills-docs", name: "Docs and research", sizeGb: 0.1, description: "Summaries, changelogs, and documentation skills." },
    { id: "skills-data", name: "Data and notebooks", sizeGb: 0.2, description: "CSV, SQL, and notebook skills." },
  ],
  mcp: [
    { id: "mcp-filesystem", name: "Filesystem", sizeGb: 0.05, description: "Scoped file access inside approved workspaces." },
    { id: "mcp-git", name: "Git", sizeGb: 0.05, description: "Local history, diffs, and branches." },
    { id: "mcp-github", name: "GitHub", sizeGb: 0.1, description: "Issues and pull requests; needs your own token later." },
  ],
  toolchains: [
    { id: "node", name: "Node.js 22 LTS", sizeGb: 0.7, description: "JavaScript and TypeScript projects." },
    { id: "python", name: "Python 3.12", sizeGb: 3.1, description: "Python with uv and common scientific wheels." },
    { id: "rust", name: "Rust stable", sizeGb: 5.4, description: "Compiler, Cargo, and rust-analyzer." },
  ],
};

const GROUP_IDS = Object.keys(catalog) as ComponentGroupId[];

function requiredIds(group: ComponentGroupId): string[] {
  return catalog[group].filter((item) => item.required).map((item) => item.id);
}

function allIds(group: ComponentGroupId): string[] {
  return catalog[group].map((item) => item.id);
}

function withRequired(components: Partial<Record<ComponentGroupId, readonly string[]>>): Record<ComponentGroupId, readonly string[]> {
  const entries = GROUP_IDS.map((group) => [group, [...new Set([...requiredIds(group), ...(components[group] ?? [])])]]);
  return Object.fromEntries(entries) as Record<ComponentGroupId, readonly string[]>;
}

export const profilePresets: Readonly<Record<Exclude<ProfileId, "custom">, Record<ComponentGroupId, readonly string[]>>> = {
  lite: withRequired({ models: ["gemma-4-e4b"], backends: ["vulkan"] }),
  developer: withRequired({
    models: allIds("models"),
    backends: ["cuda", "vulkan"],
    agents: allIds("agents"),
    skills: allIds("skills"),
    mcp: allIds("mcp"),
    toolchains: allIds("toolchains"),
  }),
};

/** Component bundles a custom profile can switch on or off; details are refined in later steps. */
export const customParts: readonly { id: string; title: string; description: string; group: ComponentGroupId; ids: readonly string[] }[] = [
  { id: "coding-models", title: "Additional coding models", description: "Qwen2.5 Coder 7B and Llama 3.2 3B.", group: "models", ids: ["qwen-coder-7b", "llama-3b"] },
  { id: "gpu", title: "GPU acceleration", description: "CUDA and Vulkan backends for faster inference.", group: "backends", ids: ["cuda", "vulkan"] },
  { id: "agents", title: "External agent adapters", description: "Codex CLI, Claude Code, and OMP.", group: "agents", ids: ["codex-cli", "claude-code", "omp"] },
  { id: "skills", title: "Skill packs", description: "Coding, docs, and data skill packs.", group: "skills", ids: allIds("skills") },
  { id: "mcp", title: "MCP servers", description: "Filesystem, Git, and GitHub servers.", group: "mcp", ids: allIds("mcp") },
  { id: "toolchains", title: "Developer toolchains", description: "Node.js, Python, and Rust.", group: "toolchains", ids: allIds("toolchains") },
];

export const initialSelections: SetupSelections = {
  driveId: null,
  profile: "lite",
  components: profilePresets.lite,
  ownerName: "",
  recoveryKeySaved: false,
};

export function driveById(id: string | null): Drive | undefined {
  return sampleDrives.find((drive) => drive.id === id);
}

export function selectedItems(selections: SetupSelections, group: ComponentGroupId): SetupComponent[] {
  return catalog[group].filter((item) => selections.components[group].includes(item.id));
}

export function groupSizeGb(components: SetupSelections["components"], group: ComponentGroupId): number {
  return catalog[group].reduce((sum, item) => (components[group].includes(item.id) ? sum + item.sizeGb : sum), 0);
}

export function estimateGb(components: SetupSelections["components"]): number {
  return GROUP_IDS.reduce((sum, group) => sum + groupSizeGb(components, group), CORE_SIZE_GB + RUNTIME.sizeGb);
}

export function formatGb(value: number): string {
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} GB`;
}

/** Returns new selections with one component switched; required components never change. */
export function toggleComponent(selections: SetupSelections, group: ComponentGroupId, id: string, on: boolean): SetupSelections {
  if (requiredIds(group).includes(id)) return selections;
  const current = selections.components[group].filter((value) => value !== id);
  const next = on ? [...current, id] : current;
  return { ...selections, profile: "custom", components: { ...selections.components, [group]: next } };
}

export function setPart(selections: SetupSelections, partId: string, on: boolean): SetupSelections {
  const part = customParts.find((candidate) => candidate.id === partId);
  if (!part) return selections;
  const rest = selections.components[part.group].filter((value) => !part.ids.includes(value));
  const next = on ? [...rest, ...part.ids] : rest;
  return { ...selections, components: { ...selections.components, [part.group]: next } };
}

export function partEnabled(selections: SetupSelections, partId: string): boolean {
  const part = customParts.find((candidate) => candidate.id === partId);
  return Boolean(part?.ids.some((id) => selections.components[part.group].includes(id)));
}

export function applyProfile(selections: SetupSelections, profile: ProfileId): SetupSelections {
  if (profile === "custom") return { ...selections, profile };
  return { ...selections, profile, components: profilePresets[profile] };
}

export function hasChatModel(selections: SetupSelections): boolean {
  return selectedItems(selections, "models").some((model) => !model.required);
}
