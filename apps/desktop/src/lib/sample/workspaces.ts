// Sample workspaces shared by every screen, so sidebars, lists, and history tell one consistent story.
import {
  BracesIcon,
  CircleDollarIcon,
  FolderIcon,
  GridTileIcon,
  LayersIcon,
  type IconProps,
} from "@aidex/ui/icons";
import type { ComponentType } from "react";

export type SampleWorkspace = {
  id: string;
  name: string;
  /** Where the project lives: on this computer or on the portable AiDEX drive. */
  location: "host" | "drive";
  path: string;
  branch: string;
  lastOpened: string;
  pinned: boolean;
  languages: readonly string[];
  Icon: ComponentType<IconProps>;
  iconTone?: "accent" | "success";
};

export const sampleWorkspaces: readonly SampleWorkspace[] = [
  {
    id: "ai-dex",
    name: "ai-dex",
    location: "drive",
    path: "AiDEX:/workspaces/ai-dex",
    branch: "main",
    lastOpened: "2 hours ago",
    pinned: true,
    languages: ["Rust", "TypeScript"],
    Icon: GridTileIcon,
    iconTone: "accent",
  },
  {
    id: "lattice",
    name: "lattice",
    location: "drive",
    path: "AiDEX:/workspaces/lattice",
    branch: "feature/graph-sync",
    lastOpened: "Yesterday",
    pinned: true,
    languages: ["Go"],
    Icon: LayersIcon,
  },
  {
    id: "tony",
    name: "tony",
    location: "drive",
    path: "AiDEX:/workspaces/tony",
    branch: "main",
    lastOpened: "3 days ago",
    pinned: true,
    languages: ["Python"],
    Icon: BracesIcon,
  },
  {
    id: "finapp",
    name: "FinApp",
    location: "host",
    path: "~/Projects/FinApp",
    branch: "feature/auth",
    lastOpened: "Today",
    pinned: false,
    languages: ["TypeScript", "React"],
    Icon: CircleDollarIcon,
    iconTone: "success",
  },
  {
    id: "oddwriter",
    name: "OddWriter",
    location: "host",
    path: "~/Projects/OddWriter",
    branch: "main",
    lastOpened: "4 days ago",
    pinned: false,
    languages: ["TypeScript"],
    Icon: FolderIcon,
  },
  {
    id: "z0d1ak",
    name: "z0d1ak",
    location: "host",
    path: "~/Projects/z0d1ak",
    branch: "develop",
    lastOpened: "Last week",
    pinned: false,
    languages: ["Rust"],
    Icon: FolderIcon,
  },
  {
    id: "olympus",
    name: "Olympus",
    location: "host",
    path: "~/Projects/Olympus",
    branch: "main",
    lastOpened: "2 weeks ago",
    pinned: false,
    languages: ["Python", "Docker"],
    Icon: FolderIcon,
  },
];

export const workspaceById = (id: string | undefined): SampleWorkspace =>
  sampleWorkspaces.find((workspace) => workspace.id === id) ?? sampleWorkspaces[3];
