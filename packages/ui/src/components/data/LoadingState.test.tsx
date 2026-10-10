import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { LoadingState } from "./LoadingState";

describe("LoadingState", () => {
  it("announces what is loading as a polite status", () => {
    render(<LoadingState label="Loading workspaces…" />);

    expect(screen.getByRole("status")).toHaveTextContent("Loading workspaces…");
  });

  it("keeps the spinner away from assistive technology", () => {
    render(<LoadingState label="Reading events…" />);

    const spinner = screen.getByRole("status").querySelector("svg");

    expect(spinner).toHaveAttribute("aria-hidden", "true");
  });
});
