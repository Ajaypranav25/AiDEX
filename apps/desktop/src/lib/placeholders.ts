// Static stand-ins for the launch flow and account until AiDEX Core exposes drives and host detection.
// Workspace and session samples live in ./sample. Nothing here is read from, or written to, a drive.
import {
  BoxIcon,
  CpuIcon,
  MemoryIcon,
  MonitorIcon,
  type IconProps,
} from "@aidex/ui/icons";
import type { ComponentType } from "react";

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
