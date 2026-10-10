import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { AsyncContent, type AsyncContentProps } from "./AsyncContent";

const renderContent = (props: Partial<AsyncContentProps>) =>
  render(
    <AsyncContent state="loading" loadingLabel="Loading workspaces…" {...props}>
      <p>api, web</p>
    </AsyncContent>,
  );

describe("AsyncContent", () => {
  it("shows what is loading and marks the region busy", () => {
    renderContent({ state: "loading" });

    expect(screen.getByRole("status")).toHaveTextContent("Loading workspaces…");
    expect(screen.getByRole("status").parentElement).toHaveAttribute("aria-busy", "true");
    expect(screen.queryByText("api, web")).not.toBeInTheDocument();
  });

  it("shows the content once ready", () => {
    renderContent({ state: "ready" });

    expect(screen.getByText("api, web")).toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("announces a failure as an alert and retries on request", async () => {
    const user = userEvent.setup();
    const onRetry = vi.fn();
    renderContent({ state: "failed", failure: "Cannot list workspaces: AiDEX Core is not running.", onRetry });

    await user.click(screen.getByRole("button", { name: "Try again" }));

    expect(screen.getByRole("alert")).toHaveTextContent("Cannot list workspaces: AiDEX Core is not running.");
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it("names the retry after the failure when several can show at once", () => {
    renderContent({ state: "failed", failure: "Cannot load provider presets", onRetry: vi.fn(), retryLabel: "Try again: presets" });

    expect(screen.getByRole("button", { name: "Try again: presets" })).toHaveTextContent("Try again");
  });

  it("offers no retry without a way to ask again", () => {
    renderContent({ state: "failed", failure: "Cannot list workspaces" });

    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  it("keeps keyboard focus in the region while the retry replaces its button", async () => {
    const user = userEvent.setup();
    let finish!: () => void;
    function Retrying() {
      const [state, setState] = useState<AsyncContentProps["state"]>("failed");
      finish = () => setState("ready");
      return (
        <AsyncContent state={state} loadingLabel="Loading workspaces…" failure="Cannot list workspaces" onRetry={() => setState("loading")}>
          <p>api, web</p>
        </AsyncContent>
      );
    }
    render(<Retrying />);
    screen.getByRole("button", { name: "Try again" }).focus();

    await user.keyboard("{Enter}");

    const region = document.activeElement;
    expect(region).not.toBe(document.body);
    expect(region).toContainElement(screen.getByRole("status"));

    act(() => finish());

    expect(document.activeElement).toBe(region);
    expect(region).toContainElement(screen.getByText("api, web"));
  });
});
