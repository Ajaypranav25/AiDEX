import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { describe, expect, it, vi } from "vitest";
import { command, isTauri, listenTo } from "./bridge";
import { ErrorKind } from "./errors";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const insideTauri = () => vi.stubGlobal("__TAURI_INTERNALS__", {});

describe("isTauri", () => {
  it("is false in a plain browser and true inside a Tauri window", () => {
    expect(isTauri()).toBe(false);

    insideTauri();

    expect(isTauri()).toBe(true);
  });
});

describe("command", () => {
  it("rejects with bridge_unavailable outside Tauri without invoking anything", async () => {
    await expect(command("aidex_status")).rejects.toMatchObject({ kind: ErrorKind.bridgeUnavailable });
    expect(invoke).not.toHaveBeenCalled();
  });

  it("invokes the named command with its arguments", async () => {
    insideTauri();
    vi.mocked(invoke).mockResolvedValue({ ok: true });

    await expect(command("select_drive", { path: "H:\\" })).resolves.toEqual({ ok: true });
    expect(invoke).toHaveBeenCalledWith("select_drive", { path: "H:\\" });
  });

  it("turns a rejected bridge payload into an AidexError", async () => {
    insideTauri();
    vi.mocked(invoke).mockRejectedValue({ code: "not_found", kind: ErrorKind.noDrive, message: "No drive." });

    await expect(command("drive_status")).rejects.toMatchObject({
      name: "AidexError",
      code: "not_found",
      kind: ErrorKind.noDrive,
      message: "No drive.",
    });
  });
});

describe("listenTo", () => {
  it("rejects with bridge_unavailable outside Tauri", async () => {
    await expect(listenTo("aidex://notification", vi.fn())).rejects.toMatchObject({
      kind: ErrorKind.bridgeUnavailable,
    });
    expect(listen).not.toHaveBeenCalled();
  });

  it("hands each event's payload to the handler", async () => {
    insideTauri();
    const unlisten = vi.fn();
    let deliver: EventCallback<unknown> | undefined;
    vi.mocked(listen).mockImplementation(async (_event, handler) => {
      deliver = handler as EventCallback<unknown>;
      return unlisten;
    });
    const handler = vi.fn();

    const stop = await listenTo("aidex://notification", handler);
    deliver?.({ event: "aidex://notification", id: 1, payload: { data: 7 } });

    expect(listen).toHaveBeenCalledWith("aidex://notification", expect.any(Function));
    expect(handler).toHaveBeenCalledWith({ data: 7 });
    expect(stop).toBe(unlisten);
  });
});
