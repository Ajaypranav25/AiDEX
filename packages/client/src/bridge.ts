import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { AidexError, toAidexError } from "./errors";

/** Whether the page runs inside a Tauri window (commands and events are reachable). */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Invoke a Tauri command; rejections become `AidexError`. Outside Tauri it rejects with `bridge_unavailable`. */
export async function command<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) throw AidexError.bridgeUnavailable();
  try {
    return await invoke<T>(name, args);
  } catch (error) {
    throw toAidexError(error);
  }
}

/** Listen to a Tauri event emitted by the Rust side. */
export async function listenTo<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  if (!isTauri()) throw AidexError.bridgeUnavailable();
  return listen<T>(event, (message) => handler(message.payload));
}
