// Mirrors `BridgeError` (apps/desktop/src-tauri/src/error.rs, apps/installer/src-tauri/src/error.rs).
// `code` is the cross-service `aidex_core::ServiceErrorCode`; `kind` is the finer stable name:
// a daemon error-table entry (crates/aidexd/src/errors.rs), a drive error kind, or a bridge kind.

/** `aidex_core::ServiceErrorCode` in snake_case. */
export type ServiceErrorCode =
  | "invalid_argument"
  | "not_found"
  | "permission_denied"
  | "conflict"
  | "incompatible_version"
  | "unavailable"
  | "internal";

/** The serialized error every Tauri command rejects with. */
export type BridgeErrorPayload = {
  code: ServiceErrorCode;
  kind: string;
  rpc_code?: number;
  message: string;
  data?: unknown;
};

/** Kinds screens branch on. Daemon error-table names are listed in protocol/commands/README.md. */
export const ErrorKind = {
  /** Not running inside a Tauri window (for example `next dev` in a browser). */
  bridgeUnavailable: "bridge_unavailable",
  noDrive: "no_drive",
  daemonUnavailable: "daemon_unavailable",
  daemonNotFound: "daemon_not_found",
  daemonLaunch: "daemon_launch",
  methodNotFound: "method_not_found",
  invalidParams: "invalid_params",
  incompatibleVersion: "incompatible_version",
  unauthorized: "unauthorized",
  vaultLocked: "vault_locked",
  policyDenied: "policy_denied",
  subscriptionLimit: "subscription_limit",
  missingManifest: "missing_manifest",
  migrationRequired: "migration_required",
  notRemovable: "not_removable",
} as const;

/** A failed bridge command or daemon call, with the backend's stable identifiers. */
export class AidexError extends Error {
  readonly code: ServiceErrorCode;
  readonly kind: string;
  readonly rpcCode?: number;
  readonly data?: unknown;

  constructor(payload: BridgeErrorPayload) {
    super(payload.message);
    this.name = "AidexError";
    this.code = payload.code;
    this.kind = payload.kind;
    this.rpcCode = payload.rpc_code;
    this.data = payload.data;
  }

  /** Thrown when the page is not running inside the AiDEX app, so no command can reach Rust. */
  static bridgeUnavailable(): AidexError {
    return new AidexError({
      code: "unavailable",
      kind: ErrorKind.bridgeUnavailable,
      message: "This screen is running outside the AiDEX app, so it cannot reach AiDEX Core.",
    });
  }

  is(kind: string): boolean {
    return this.kind === kind;
  }
}

function isPayload(value: unknown): value is BridgeErrorPayload {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Record<string, unknown>;
  return typeof candidate.code === "string" && typeof candidate.kind === "string" && typeof candidate.message === "string";
}

/** Normalize anything a command rejected with into an `AidexError`. */
export function toAidexError(value: unknown): AidexError {
  if (value instanceof AidexError) return value;
  if (isPayload(value)) return new AidexError(value);
  const message = value instanceof Error ? value.message : typeof value === "string" ? value : "Unknown bridge error";
  return new AidexError({ code: "internal", kind: "internal", message });
}
