import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, vi } from "vitest";
import { aidexStatus, driveStatus, sandboxCapabilities, selectDrive, startDaemon, stopDaemon } from "./desktop";
import { hostProfile } from "./host";
import { initDrive, inspectDrive, listDrives } from "./installer";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

// The names must match the `#[tauri::command]` functions registered in each app's `src-tauri`.
describe("command wrappers", () => {
  it.each([
    ["aidexStatus", () => aidexStatus(), "aidex_status", undefined],
    ["driveStatus", () => driveStatus(), "drive_status", undefined],
    ["sandboxCapabilities", () => sandboxCapabilities(), "sandbox_capabilities", undefined],
    ["startDaemon", () => startDaemon(), "daemon_start", undefined],
    ["stopDaemon", () => stopDaemon(), "daemon_stop", undefined],
    ["selectDrive", () => selectDrive("H:\\"), "select_drive", { path: "H:\\" }],
    ["hostProfile", () => hostProfile(), "host_profile", undefined],
    ["listDrives", () => listDrives(), "list_drives", undefined],
    ["inspectDrive", () => inspectDrive("E:\\"), "inspect_drive", { root: "E:\\" }],
    ["initDrive", () => initDrive("E:\\"), "init_drive", { root: "E:\\" }],
  ])("%s invokes %s", async (_name, run, commandName, args) => {
    vi.stubGlobal("__TAURI_INTERNALS__", {});
    vi.mocked(invoke).mockResolvedValue("result");

    await expect(run()).resolves.toBe("result");
    expect(invoke).toHaveBeenCalledWith(commandName, args);
  });
});
