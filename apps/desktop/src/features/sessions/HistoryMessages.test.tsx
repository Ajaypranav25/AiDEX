import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ErrorKind } from "@aidex/client/errors";
import { call } from "@aidex/client/rpc";
import { aidexConnection, aidexWrapper } from "../../test/aidex";
import { HistoryMessages } from "./HistoryMessages";

vi.mock("@aidex/client/rpc", () => ({ call: vi.fn() }));

describe("HistoryMessages", () => {
  it("explains why the history cannot be read and reads it again on request", async () => {
    const user = userEvent.setup();
    vi.mocked(call)
      .mockRejectedValueOnce({ code: "unavailable", kind: ErrorKind.daemonUnavailable, message: "AiDEX Core is not running." })
      .mockResolvedValueOnce({ messages: [], offset: 0, total_messages: 0, next_offset: null, truncated: false });
    render(<HistoryMessages scope={{ session_id: "ses_1" }} />, { wrapper: aidexWrapper(() => aidexConnection()) });
    expect(await screen.findByText("Cannot read the history: AiDEX Core is not running.")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Try again" }));

    await waitFor(() => expect(screen.queryByText(/Cannot read the history/)).not.toBeInTheDocument());
    expect(call).toHaveBeenLastCalledWith("history.list", { session_id: "ses_1", offset: 0, limit: 20 });
  });
});
