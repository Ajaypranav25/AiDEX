import { modelOptions } from "../../lib/sample/models";
import { localModels, type Compatibility } from "./sample";

export const formatGb = (value: number) => `${value} GB`;

export const formatRate = (value: number) => value.toLocaleString("en-US");

/** Badge tone for a compatibility verdict: fits is good, anything else is a neutral caveat. */
export const compatibilityTone = (compatibility: Compatibility) =>
  compatibility === "Fits in VRAM" ? ("success" as const) : ("neutral" as const);

/** Display name of the model selected for new chats, whether local or a cloud provider. */
export function currentModelName(modelId: string): string {
  const local = localModels.find((model) => model.id === modelId);
  if (local) return local.name;
  return modelOptions.find((option) => option.value === modelId)?.label ?? "No model selected";
}
