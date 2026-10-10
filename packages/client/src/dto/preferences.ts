import type { PermissionMode } from "./sessions";
/** Mirrors aidex-preferences::AppPreferences; stored encrypted on the drive. */
export type AppPreferences = {
  accent: string;
  theme: "dark" | "light" | "system";
  density: "comfortable" | "compact";
  reduce_motion: boolean;
  keyboard_hints: boolean;
  model_id: string;
  workspace_id: string;
  permission_mode: PermissionMode | null;
  instructions: string;
  fallback_model: string | null;
  max_steps: number;
  max_tokens: number;
  shortcuts: Record<string, string>;
};
