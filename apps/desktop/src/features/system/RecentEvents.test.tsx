import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { EventEnvelope } from "@aidex/client/dto/daemon";
import { ErrorKind } from "@aidex/client/errors";
import { call } from "@aidex/client/rpc";
import { aidexConnection, aidexWrapper } from "../../test/aidex";
import { RecentEvents } from "./RecentEvents";

vi.mock("@aidex/client/rpc", () => ({ call: vi.fn() }));

const event = (cursor: number, type: string): EventEnvelope => ({
  version: 2,
  cursor,
  scope: {},
  occurred_at: "2026-10-11T09:00:00Z",
  event: { type },
});

/** `events.latest` then `events.replay` answer with these events. */
function daemonHasEvents(events: EventEnvelope[]) {
  vi.mocked(call).mockImplementation(async (method) =>
    method === "events.latest" ? { latest_cursor: events.length, oldest_cursor: 1 } : { events, next_cursor: 0, has_more: false },
  );
}

const renderEvents = () => render(<RecentEvents />, { wrapper: aidexWrapper(() => aidexConnection()) });

describe("RecentEvents", () => {
  it("shows that events are being read", () => {
    vi.mocked(call).mockReturnValue(new Promise(() => {}));

    renderEvents();

    expect(screen.getByRole("status")).toHaveTextContent("Reading events…");
  });

  it("lists the newest events first", async () => {
    daemonHasEvents([event(1, "vault.unlocked"), event(2, "session.started")]);

    renderEvents();

    const entries = await screen.findAllByRole("listitem");
    expect(entries).toHaveLength(2);
    expect(entries[0]).toHaveTextContent("session started");
    expect(entries[1]).toHaveTextContent("vault unlocked");
  });

  it("announces a failure and reads the events after a retry", async () => {
    const user = userEvent.setup();
    vi.mocked(call).mockRejectedValueOnce({
      code: "unavailable",
      kind: ErrorKind.daemonUnavailable,
      message: "AiDEX Core is not running.",
    });
    renderEvents();
    expect(await screen.findByRole("alert")).toHaveTextContent("Cannot read events: AiDEX Core is not running.");

    daemonHasEvents([event(1, "vault.unlocked")]);
    await user.click(screen.getByRole("button", { name: "Try again: Cannot read events" }));

    expect(await screen.findByText("vault unlocked")).toBeInTheDocument();
  });
});
