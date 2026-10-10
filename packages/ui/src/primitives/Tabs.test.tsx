import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { Tabs } from "./Tabs";

const items = [
  { value: "overview", label: "Overview" },
  { value: "files", label: "Files" },
  { value: "git", label: "Git" },
] as const;

type Value = (typeof items)[number]["value"];

function ControlledTabs({ onValueChange }: { onValueChange?: (value: Value) => void }) {
  const [value, setValue] = useState<Value>("overview");
  return (
    <Tabs
      id="workspace"
      label="Workspace sections"
      items={items}
      value={value}
      onValueChange={(next) => {
        setValue(next);
        onValueChange?.(next);
      }}
    />
  );
}

const tab = (name: string) => screen.getByRole("tab", { name });

describe("Tabs", () => {
  it("exposes one tab stop: the selected tab, linked to its panel", () => {
    render(<ControlledTabs />);

    expect(screen.getByRole("tablist", { name: "Workspace sections" })).toBeInTheDocument();
    expect(tab("Overview")).toHaveAttribute("aria-selected", "true");
    expect(tab("Overview")).toHaveAttribute("tabindex", "0");
    expect(tab("Overview")).toHaveAttribute("aria-controls", "workspace-panel-overview");
    expect(tab("Files")).toHaveAttribute("tabindex", "-1");
    expect(tab("Files")).not.toHaveAttribute("aria-controls");
  });

  it("selects a tab on click", async () => {
    const user = userEvent.setup();
    const onValueChange = vi.fn();
    render(<ControlledTabs onValueChange={onValueChange} />);

    await user.click(tab("Git"));

    expect(onValueChange).toHaveBeenCalledWith("git");
    expect(tab("Git")).toHaveAttribute("aria-selected", "true");
  });

  it("moves selection and focus with the arrow keys, wrapping at both ends", async () => {
    const user = userEvent.setup();
    render(<ControlledTabs />);
    tab("Overview").focus();

    await user.keyboard("{ArrowLeft}");
    expect(tab("Git")).toHaveFocus();
    expect(tab("Git")).toHaveAttribute("aria-selected", "true");

    await user.keyboard("{ArrowRight}");
    expect(tab("Overview")).toHaveFocus();

    await user.keyboard("{ArrowRight}");
    expect(tab("Files")).toHaveFocus();
  });

  it("jumps to the first and last tab with Home and End", async () => {
    const user = userEvent.setup();
    render(<ControlledTabs />);
    tab("Overview").focus();

    await user.keyboard("{End}");
    expect(tab("Git")).toHaveFocus();

    await user.keyboard("{Home}");
    expect(tab("Overview")).toHaveFocus();
  });

  it("leaves arrow keys with a modifier to the browser and assistive technology", async () => {
    const user = userEvent.setup();
    const onValueChange = vi.fn();
    render(<ControlledTabs onValueChange={onValueChange} />);
    tab("Overview").focus();

    await user.keyboard("{Alt>}{ArrowRight}{/Alt}");

    expect(onValueChange).not.toHaveBeenCalled();
    expect(tab("Overview")).toHaveFocus();
  });
});
