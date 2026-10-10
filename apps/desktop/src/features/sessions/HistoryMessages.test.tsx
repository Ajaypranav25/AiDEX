import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Message } from "@aidex/client/dto/sessions";
import { ErrorKind } from "@aidex/client/errors";
import { call } from "@aidex/client/rpc";
import { aidexConnection, aidexWrapper } from "../../test/aidex";
import { HistoryMessages } from "./HistoryMessages";

vi.mock("@aidex/client/rpc", () => ({ call: vi.fn() }));

const message: Message = {
  original_index: 1,
  role: "user",
  content: "Explain the vault layout",
  content_bytes: 24,
  content_truncated: false,
  tool_calls: [],
  tool_calls_total: 0,
  tool_calls_truncated: false,
  tool_call_id: null,
};

const renderHistory = () =>
  render(<HistoryMessages scope={{ session_id: "ses_1" }} />, { wrapper: aidexWrapper(() => aidexConnection()) });

describe("HistoryMessages", () => {
  it("shows that the messages are loading", () => {
    vi.mocked(call).mockReturnValue(new Promise(() => {}));

    renderHistory();

    expect(screen.getByRole("status")).toHaveTextContent("Loading messages…");
  });

  it("announces why the history cannot be read and shows it after a retry", async () => {
    const user = userEvent.setup();
    vi.mocked(call)
      .mockRejectedValueOnce({ code: "unavailable", kind: ErrorKind.daemonUnavailable, message: "AiDEX Core is not running." })
      .mockResolvedValueOnce({ messages: [message], offset: 0, total_messages: 1, next_offset: null, truncated: false });
    renderHistory();
    expect(await screen.findByRole("alert")).toHaveTextContent("Cannot read the history: AiDEX Core is not running.");

    await user.click(screen.getByRole("button", { name: "Try again: Cannot read the history" }));

    expect(await screen.findByText("Explain the vault layout")).toBeInTheDocument();
    expect(call).toHaveBeenLastCalledWith("history.list", { session_id: "ses_1", offset: 0, limit: 20 });
  });
});
