import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { driveStatus } from "@aidex/client/desktop";
import { ErrorKind } from "@aidex/client/errors";
import { call } from "@aidex/client/rpc";
import { aidexConnection, aidexWrapper } from "../../test/aidex";
import { UnlockPage } from "./UnlockPage";

vi.mock("@aidex/client/rpc", () => ({ call: vi.fn() }));
vi.mock("@aidex/client/desktop", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@aidex/client/desktop")>()),
  driveStatus: vi.fn(),
}));

function renderUnlock() {
  vi.mocked(driveStatus).mockReturnValue(new Promise(() => {}));
  return render(<UnlockPage onContinue={vi.fn()} />, { wrapper: aidexWrapper(() => aidexConnection()) });
}

describe("UnlockPage vault status", () => {
  it("shows that the vault is being checked", () => {
    vi.mocked(call).mockReturnValue(new Promise(() => {}));

    renderUnlock();

    expect(screen.getByRole("status")).toHaveTextContent("Checking the vault…");
  });

  it("announces why the vault status cannot be read and asks again on request", async () => {
    const user = userEvent.setup();
    vi.mocked(call)
      .mockRejectedValueOnce({ code: "internal", kind: "internal", message: "The vault file is busy." })
      .mockResolvedValueOnce({ state: "locked", vault_id: "vlt_1", has_recovery_key: true });
    renderUnlock();
    expect(await screen.findByRole("alert")).toHaveTextContent("Cannot read the vault status: The vault file is busy.");

    await user.click(screen.getByRole("button", { name: "Try again: Cannot read the vault status" }));

    expect(await screen.findByLabelText("Workspace password")).toBeInTheDocument();
    expect(screen.queryByText(/Cannot read the vault status/)).not.toBeInTheDocument();
  });

  it("treats a Core without a vault service as nothing to unlock", async () => {
    vi.mocked(call).mockRejectedValue({ code: "not_found", kind: ErrorKind.methodNotFound, message: "Unknown method." });

    renderUnlock();

    expect(await screen.findByText("Vault not available")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Try again/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Continue" })).toBeInTheDocument();
  });
});
