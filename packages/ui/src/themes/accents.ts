/** Accent presets. Each keeps dark `--aidex-color-on-accent` text readable on the filled accent. */
export const accentPresets = [
  { value: "#1a8cff", label: "Electric blue" },
  { value: "#8b7bff", label: "Violet" },
  { value: "#10c77e", label: "Green" },
  { value: "#f2a93b", label: "Amber" },
  { value: "#ff6b8a", label: "Rose" },
  { value: "#2ec5d3", label: "Cyan" },
] as const;

export const DEFAULT_ACCENT = accentPresets[0].value;

/**
 * Applies an accent color at runtime. The other accent tokens (border, soft, ring, focus) are
 * derived from `--aidex-color-accent` in tokens.css, so this single property re-themes them all.
 */
export function applyAccent(color: string, root: HTMLElement = document.documentElement): void {
  root.style.setProperty("--aidex-color-accent", color);
}
