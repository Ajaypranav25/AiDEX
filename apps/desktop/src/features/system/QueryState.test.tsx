import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AidexError, ErrorKind } from "@aidex/client/errors";
import type { BridgeQuery } from "../../hooks/useBridgeQuery";
import { QueryState } from "./QueryState";

function query<T>(overrides: Partial<BridgeQuery<T>> = {}): BridgeQuery<T> {
  return { data: undefined, error: null, loading: true, settled: false, refetch: vi.fn(), ...overrides };
}

const daemonDown = new AidexError({
  code: "unavailable",
  kind: ErrorKind.daemonUnavailable,
  message: "AiDEX Core is not running.",
});

function Names({ query: current }: { query: BridgeQuery<string[]> }) {
  return (
    <QueryState query={current} loading="Loading workspaces…" failure="Cannot list workspaces">
      {(names) => <p>{names.join(", ")}</p>}
    </QueryState>
  );
}

describe("QueryState", () => {
  it("shows what is loading until the first answer arrives", () => {
    render(<Names query={query()} />);

    expect(screen.getByRole("status")).toHaveTextContent("Loading workspaces…");
  });

  it("renders the data once it has arrived", () => {
    render(<Names query={query({ data: ["api", "web"], loading: false, settled: true })} />);

    expect(screen.getByText("api, web")).toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("announces a failure and retries on request", async () => {
    const user = userEvent.setup();
    const failed = query<string[]>({ error: daemonDown, loading: false, settled: true });
    render(<Names query={failed} />);

    await user.click(screen.getByRole("button", { name: "Try again: Cannot list workspaces" }));

    expect(screen.getByRole("alert")).toHaveTextContent("Cannot list workspaces: AiDEX Core is not running.");
    expect(failed.refetch).toHaveBeenCalledTimes(1);
  });

  it("keeps keyboard focus in place from the retry to the answer", async () => {
    const user = userEvent.setup();
    const { rerender } = render(<Names query={query({ error: daemonDown, loading: false, settled: true })} />);
    screen.getByRole("button", { name: /Try again/ }).focus();

    await user.keyboard("{Enter}");
    const region = document.activeElement;
    rerender(<Names query={query()} />);
    rerender(<Names query={query({ data: ["api"], loading: false, settled: true })} />);

    expect(region).not.toBe(document.body);
    expect(document.activeElement).toBe(region);
    expect(region).toContainElement(screen.getByText("api"));
  });

  it("renders nothing while the query is disabled", () => {
    const { container } = render(<Names query={query({ loading: false })} />);

    expect(container).toBeEmptyDOMElement();
  });
});
