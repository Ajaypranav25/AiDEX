// Sample data until AiDEX Core exposes the models on the drive, runtime detection, benchmarks, and provider accounts.

export type Compatibility = "Fits in VRAM" | "CPU only" | "Needs 12 GB VRAM";

export type Quantization = {
  name: string;
  sizeGb: number;
  ramGb: number;
  quality: string;
  tokensPerSecond: number;
  installed: boolean;
};

export type LocalModel = {
  /** Route id; chat models share their id with `modelOptions` so "Use this model" updates the top bar. */
  id: string;
  name: string;
  kind: "chat" | "embedding";
  quantization: string;
  sizeGb: number;
  compatibility: Compatibility;
  summary: string;
  stats: { ramGb: number; vramGb: number; tokensPerSecond: number; context: string; contextHint: string };
  quantizations: readonly Quantization[];
  hardware: readonly { label: string; tokensPerSecond: number }[];
  details: { architecture: string; parameters: string; license: string; source: string; checksum: string; added: string };
};

export const localModels: readonly LocalModel[] = [
  {
    id: "gemma-4",
    name: "Gemma 4 E4B",
    kind: "chat",
    quantization: "Q4_K_M",
    sizeGb: 3.2,
    compatibility: "Fits in VRAM",
    summary: "General chat, writing, and light coding",
    stats: { ramGb: 5.1, vramGb: 3.9, tokensPerSecond: 42.1, context: "128K", contextHint: "AiDEX default 8K" },
    quantizations: [
      { name: "Q4_K_M", sizeGb: 3.2, ramGb: 5.1, quality: "Balanced", tokensPerSecond: 42.1, installed: true },
      { name: "Q5_K_M", sizeGb: 3.7, ramGb: 5.6, quality: "Higher", tokensPerSecond: 37.4, installed: false },
      { name: "Q8_0", sizeGb: 5.4, ramGb: 7.3, quality: "Near lossless", tokensPerSecond: 28.9, installed: false },
    ],
    hardware: [
      { label: "GPU · CUDA 12", tokensPerSecond: 42.1 },
      { label: "GPU · Vulkan", tokensPerSecond: 33.8 },
      { label: "CPU · 8 threads", tokensPerSecond: 9.6 },
    ],
    details: {
      architecture: "Gemma 4 · decoder-only transformer",
      parameters: "4B effective (8B total)",
      license: "Gemma Terms of Use",
      source: "Imported from Hugging Face",
      checksum: "9f2c41d8…a71e",
      added: "Sep 28, 2026",
    },
  },
  {
    id: "qwen-coder-7b",
    name: "Qwen2.5 Coder 7B",
    kind: "chat",
    quantization: "Q4_K_M",
    sizeGb: 4.7,
    compatibility: "Fits in VRAM",
    summary: "Code generation, review, and agent runs",
    stats: { ramGb: 6.4, vramGb: 5.3, tokensPerSecond: 31.6, context: "32K", contextHint: "AiDEX default 16K" },
    quantizations: [
      { name: "Q4_K_M", sizeGb: 4.7, ramGb: 6.4, quality: "Balanced", tokensPerSecond: 31.6, installed: true },
      { name: "Q5_K_M", sizeGb: 5.4, ramGb: 7.1, quality: "Higher", tokensPerSecond: 27.9, installed: false },
      { name: "Q8_0", sizeGb: 8.1, ramGb: 9.8, quality: "Near lossless", tokensPerSecond: 19.8, installed: false },
    ],
    hardware: [
      { label: "GPU · CUDA 12", tokensPerSecond: 31.6 },
      { label: "GPU · Vulkan", tokensPerSecond: 25.2 },
      { label: "CPU · 8 threads", tokensPerSecond: 6.1 },
    ],
    details: {
      architecture: "Qwen2 · decoder-only transformer",
      parameters: "7.6B",
      license: "Apache 2.0",
      source: "Imported from Hugging Face",
      checksum: "3b7e09c2…5d14",
      added: "Sep 30, 2026",
    },
  },
  {
    id: "llama-3.2-3b",
    name: "Llama 3.2 3B",
    kind: "chat",
    quantization: "Q4_K_M",
    sizeGb: 2.0,
    compatibility: "Fits in VRAM",
    summary: "Fast replies and summaries on modest hardware",
    stats: { ramGb: 3.1, vramGb: 2.4, tokensPerSecond: 58.3, context: "128K", contextHint: "AiDEX default 8K" },
    quantizations: [
      { name: "Q4_K_M", sizeGb: 2.0, ramGb: 3.1, quality: "Balanced", tokensPerSecond: 58.3, installed: true },
      { name: "Q5_K_M", sizeGb: 2.3, ramGb: 3.4, quality: "Higher", tokensPerSecond: 52.0, installed: false },
      { name: "Q8_0", sizeGb: 3.4, ramGb: 4.6, quality: "Near lossless", tokensPerSecond: 40.6, installed: false },
    ],
    hardware: [
      { label: "GPU · CUDA 12", tokensPerSecond: 58.3 },
      { label: "GPU · Vulkan", tokensPerSecond: 47.5 },
      { label: "CPU · 8 threads", tokensPerSecond: 14.2 },
    ],
    details: {
      architecture: "Llama 3.2 · decoder-only transformer",
      parameters: "3.2B",
      license: "Llama 3.2 Community License",
      source: "Bundled with AiDEX",
      checksum: "c81a6f30…02be",
      added: "Aug 14, 2026",
    },
  },
  {
    id: "nomic-embed-text",
    name: "nomic-embed-text v1.5",
    kind: "embedding",
    quantization: "F16",
    sizeGb: 0.3,
    compatibility: "CPU only",
    summary: "Embeddings for workspace memory and retrieval",
    stats: { ramGb: 0.6, vramGb: 0, tokensPerSecond: 1850, context: "8K", contextHint: "Chunks of 512 tokens" },
    quantizations: [
      { name: "Q4_K_M", sizeGb: 0.1, ramGb: 0.3, quality: "Lower recall", tokensPerSecond: 2400, installed: false },
      { name: "Q8_0", sizeGb: 0.15, ramGb: 0.4, quality: "Near lossless", tokensPerSecond: 2100, installed: false },
      { name: "F16", sizeGb: 0.3, ramGb: 0.6, quality: "Reference", tokensPerSecond: 1850, installed: true },
    ],
    hardware: [
      { label: "GPU · CUDA 12", tokensPerSecond: 6200 },
      { label: "GPU · Vulkan", tokensPerSecond: 4800 },
      { label: "CPU · 8 threads", tokensPerSecond: 1850 },
    ],
    details: {
      architecture: "nomic-bert · encoder",
      parameters: "137M",
      license: "Apache 2.0",
      source: "Bundled with AiDEX",
      checksum: "e4d920ab…9c37",
      added: "Aug 14, 2026",
    },
  },
];

export function localModelById(id: string | undefined): LocalModel {
  return localModels.find((model) => model.id === id) ?? localModels[0];
}

export const downloadableModels = [
  {
    id: "gemma-4-12b",
    name: "Gemma 4 12B",
    detail: "Q4_K_M · 7.3 GB",
    compatibility: "Needs 12 GB VRAM",
    description: "Stronger reasoning and longer answers than E4B; runs partly on CPU with 8 GB of VRAM.",
  },
  {
    id: "phi-4-mini",
    name: "Phi-4 mini",
    detail: "Q4_K_M · 2.5 GB",
    compatibility: "Fits in VRAM",
    description: "Small instruction model for quick answers, math, and structured output.",
  },
  {
    id: "deepseek-coder-v2-lite",
    name: "DeepSeek-Coder-V2 Lite",
    detail: "Q4_K_M · 10.4 GB",
    compatibility: "Needs 12 GB VRAM",
    description: "Mixture-of-experts coding model with a 128K context for large repositories.",
  },
] as const;

export const runtimeInfo = [
  { label: "Engine", value: "llama.cpp b6710" },
  { label: "Acceleration", value: "CUDA 12 · Vulkan · CPU" },
  { label: "Mode", value: "Offline supported" },
  { label: "Model source", value: "AiDEX drive" },
] as const;

export const driveCapacityGb = 1000;

export type ProviderAccount = { id: string; name: string; description: string; connected: boolean };

export const providerAccounts: readonly ProviderAccount[] = [
  { id: "openai", name: "OpenAI", description: "GPT models via the OpenAI API", connected: true },
  { id: "anthropic", name: "Anthropic", description: "Claude models via the Anthropic API", connected: false },
  { id: "gemini", name: "Google Gemini", description: "Gemini models via the Gemini API", connected: false },
  { id: "compatible", name: "OpenAI-compatible endpoint", description: "Self-hosted or third-party server", connected: false },
];
