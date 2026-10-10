// Static stand-ins for the launch flow and home dashboard until AiDEX Core exposes drives,
// host detection, sessions, and workspaces. Nothing here is read from, or written to, a drive.
import {
  BoxIcon,
  BracesIcon,
  CircleDollarIcon,
  CpuIcon,
  FolderIcon,
  GridTileIcon,
  LayersIcon,
  MemoryIcon,
  MonitorIcon,
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

export const detectedDrive = { name: "Samsung T7", description: "Portable SSD · 1 TB" };

export type HostCheck = {
  id: string;
  label: string;
  value: string;
  detail: string;
  status: "Detected" | "Compatible";
  Icon: ComponentType<IconProps>;
};

export const hostChecks: readonly HostCheck[] = [
  { id: "cpu", label: "CPU", value: "Apple M2 Pro", detail: "12 cores (8P + 4E)", status: "Detected", Icon: CpuIcon },
  { id: "gpu", label: "GPU", value: "Apple M2 Pro (19-core)", detail: "Metal 3 · 16 GB", status: "Compatible", Icon: MonitorIcon },
  { id: "ram", label: "RAM", value: "32 GB", detail: "Unified Memory", status: "Detected", Icon: MemoryIcon },
  {
    id: "model",
    label: "Local Model",
    value: "Llama 3.2 3B Instruct (Q4)",
    detail: "Optimized for Apple Silicon",
    status: "Compatible",
    Icon: BoxIcon,
  },
];
