import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Notice } from "./Notice";

describe("Notice", () => {
  it("offers a trailing action after its message", async () => {
    const user = userEvent.setup();
    const retry = vi.fn();
    render(
      <Notice tone="warning" action={<button onClick={retry}>Try again</button>}>
        Cannot list workspaces: AiDEX Core is not running.
      </Notice>,
    );

    await user.click(screen.getByRole("button", { name: "Try again" }));

    expect(screen.getByText(/Cannot list workspaces/)).toBeInTheDocument();
    expect(retry).toHaveBeenCalledTimes(1);
  });

  it("renders only its message when there is no action", () => {
    render(<Notice>Showing the latest 100 checkpoints.</Notice>);

    expect(screen.getByText("Showing the latest 100 checkpoints.")).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });
});
