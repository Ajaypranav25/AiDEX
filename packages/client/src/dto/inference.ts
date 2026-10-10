export type InferenceBackend = "auto" | "cpu" | "gpu";
export type ManagedModel = {
  id: string;
  name: string;
  path: string;
  sha256: string;
  size_bytes: number;
  gguf_version: number;
  context_window: number | null;
  availability: "available" | "missing" | "corrupt";
};
export type InferenceRuntime = {
  release: string;
  platform: string;
  path: string;
  executable: string;
  verified: boolean;
  gpu_supported: boolean;
};
export type InferenceOptions = {
  model_id: string;
  backend: InferenceBackend;
  context_window?: number;
  threads?: number;
};
export type InferenceCatalog = {
  models: ManagedModel[];
  runtime: InferenceRuntime | null;
  settings: InferenceOptions | null;
};
export type InferenceStatus = {
  state: string;
  model_id: string | null;
  backend: InferenceBackend | null;
  endpoint: string | null;
  pid: number | null;
  context_window: number | null;
  threads: number | null;
  limitations: string[];
};
