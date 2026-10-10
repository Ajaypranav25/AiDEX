import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AidexError, ErrorKind } from "@aidex/client/errors";
import type { BridgeQuery } from "../../hooks/useBridgeQuery";
import { QueryError, QueryState } from "./QueryState";

function query<T>(overrides: Partial<BridgeQuery<T>> = {}): BridgeQuery<T> {
  return { data: undefined, error: null, loading: true, settled: false, refetch: vi.fn(), ...overrides };
}

const daemonDown = new AidexError({
  code: "unavailable",
  kind: ErrorKind.daemonUnavailable,
  message: "AiDEX Core is not running.",
});

describe("QueryState", () => {
  it("shows what is loading until the first answer arrives", () => {
    render(
      <QueryState query={query<string[]>()} loading="Loading workspaces…" failure="Cannot list workspaces">
        {(names) => names.join(", ")}
      </QueryState>,
    );

    expect(screen.getByRole("status")).toHaveTextContent("Loading workspaces…");
  });

  it("renders the data once it has arrived", () => {
    render(
      <QueryState
        query={query({ data: ["api", "web"], loading: false, settled: true })}
        loading="Loading workspaces…"
        failure="Cannot list workspaces"
      >
        {(names) => <p>{names.join(", ")}</p>}
      </QueryState>,
    );

    expect(screen.getByText("api, web")).toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("explains a failure and retries on request", async () => {
    const user = userEvent.setup();
    const failed = query<string[]>({ error: daemonDown, loading: false, settled: true });
    render(
      <QueryState query={failed} loading="Loading workspaces…" failure="Cannot list workspaces">
        {(names) => names.join(", ")}
      </QueryState>,
    );

    await user.click(screen.getByRole("button", { name: "Try again" }));

    expect(screen.getByText("Cannot list workspaces: AiDEX Core is not running.")).toBeInTheDocument();
    expect(failed.refetch).toHaveBeenCalledTimes(1);
  });
});

describe("QueryError", () => {
  it("renders nothing while the query has not failed", () => {
    const { container } = render(<QueryError query={query({ data: 1 })} failure="Cannot read the history" />);

    expect(container).toBeEmptyDOMElement();
  });

  it("explains a failure and retries on request", async () => {
    const user = userEvent.setup();
    const failed = query({ error: daemonDown, loading: false, settled: true });
    render(<QueryError query={failed} failure="Cannot read the history" />);

    await user.click(screen.getByRole("button", { name: "Try again" }));

    expect(screen.getByText("Cannot read the history: AiDEX Core is not running.")).toBeInTheDocument();
    expect(failed.refetch).toHaveBeenCalledTimes(1);
  });
});
