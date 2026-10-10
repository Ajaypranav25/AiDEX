import { describe, expect, it } from "vitest";
import { AidexError, ErrorKind, toAidexError } from "./errors";

describe("toAidexError", () => {
  it("returns an AidexError unchanged", () => {
    const error = AidexError.bridgeUnavailable();

    expect(toAidexError(error)).toBe(error);
  });

  it("keeps every field of a bridge error payload", () => {
    const error = toAidexError({
      code: "permission_denied",
      kind: ErrorKind.vaultLocked,
      rpc_code: -32010,
      message: "The vault is locked.",
      data: { retry: false },
    });

    expect(error).toBeInstanceOf(AidexError);
    expect(error).toMatchObject({
      name: "AidexError",
      code: "permission_denied",
      kind: "vault_locked",
      rpcCode: -32010,
      message: "The vault is locked.",
      data: { retry: false },
    });
    expect(error.is(ErrorKind.vaultLocked)).toBe(true);
    expect(error.is(ErrorKind.noDrive)).toBe(false);
  });

  it.each([
    ["an Error", new Error("socket closed"), "socket closed"],
    ["a string", "daemon crashed", "daemon crashed"],
    ["a payload without a kind", { code: "internal", message: "half a payload" }, "Unknown bridge error"],
    ["null", null, "Unknown bridge error"],
    ["a number", 42, "Unknown bridge error"],
  ])("wraps %s as an internal error", (_label, value, message) => {
    const error = toAidexError(value);

    expect(error).toMatchObject({ code: "internal", kind: "internal", message });
  });
});

describe("AidexError.bridgeUnavailable", () => {
  it("explains that the page runs outside the app", () => {
    const error = AidexError.bridgeUnavailable();

    expect(error.code).toBe("unavailable");
    expect(error.is(ErrorKind.bridgeUnavailable)).toBe(true);
    expect(error.message).toMatch(/outside the AiDEX app/);
  });
});
