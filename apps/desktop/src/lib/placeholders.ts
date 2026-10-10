// Static stand-ins for the home dashboard until AiDEX Core exposes sessions and workspaces.
// Nothing here is read from, or written to, a drive.
import {
  BracesIcon,
  CircleDollarIcon,
  FolderIcon,
  GridTileIcon,
  LayersIcon,
  type IconProps,
} from "@aidex/ui/icons";
import type { ComponentType } from "react";

export type SidebarEntry = {
  id: string;
  label: string;
  Icon?: ComponentType<IconProps>;
  tone?: "accent" | "success";
};

export const pinnedEntries: readonly SidebarEntry[] = [
  { id: "pinned-ai-dex", label: "ai-dex", Icon: GridTileIcon, tone: "accent" },
  { id: "pinned-lattice", label: "lattice", Icon: LayersIcon },
  { id: "pinned-tony", label: "tony", Icon: BracesIcon },
];

export const workspaceEntries: readonly SidebarEntry[] = [
  { id: "workspace-finapp", label: "FinApp", Icon: CircleDollarIcon, tone: "success" },
  { id: "workspace-oddwriter", label: "OddWriter", Icon: FolderIcon },
  { id: "workspace-z0d1ak", label: "z0d1ak", Icon: FolderIcon },
  { id: "workspace-olympus", label: "Olympus", Icon: FolderIcon },
];

export const recentEntries: readonly SidebarEntry[] = [
  "Design system discussion",
  "Auth flow implementation",
  "Q3 roadmap planning",
  "Bug analysis and fix",
  "Model evaluation notes",
  "Church Street timing plan",
  "Schedule email summaries",
].map((label, index) => ({ id: `recent-${index}`, label }));

export const placeholderAccount = { name: "J" };
