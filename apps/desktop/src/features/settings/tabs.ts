import type { ComponentType } from "react";
import { HardDriveIcon, KeyboardIcon, PaletteIcon, RefreshIcon, StethoscopeIcon, type IconProps } from "@aidex/ui/icons";
import type { PageTab } from "../../lib/routes";

export type SettingsTab = PageTab<"settings">;

/** Settings sections, shared by the page tabs and the sidebar so both stay in the same order. */
export const settingsTabs: readonly { value: SettingsTab; label: string; Icon: ComponentType<IconProps> }[] = [
  { value: "appearance", label: "Appearance", Icon: PaletteIcon },
  { value: "shortcuts", label: "Shortcuts", Icon: KeyboardIcon },
  { value: "updates", label: "Updates", Icon: RefreshIcon },
  { value: "diagnostics", label: "Diagnostics", Icon: StethoscopeIcon },
  { value: "drive", label: "Drive and storage", Icon: HardDriveIcon },
];
