import { render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { isApplePlatform } from "../../lib/platform";
import { ShortcutsPanel } from "./ShortcutsPanel";

vi.mock("../../lib/platform", () => ({ isApplePlatform: vi.fn(() => false) }));

const keysOf = (action: string) => {
  const row = screen.getByRole("rowheader", { name: action }).closest("tr");
  return within(row as HTMLElement)
    .getAllByText((_, element) => element?.tagName === "KBD")
    .map((key) => key.textContent);
};

describe("ShortcutsPanel", () => {
  it("lists the history shortcuts with Alt and the arrow keys outside Apple platforms", () => {
    render(<ShortcutsPanel />);

    expect(keysOf("Go back")).toEqual(["Alt", "←"]);
    expect(keysOf("Go forward")).toEqual(["Alt", "→"]);
    expect(keysOf("New session")).toEqual(["Ctrl", "K"]);
  });

  it("lists the history shortcuts with ⌘ and brackets on Apple platforms", () => {
    vi.mocked(isApplePlatform).mockReturnValue(true);

    render(<ShortcutsPanel />);

    expect(keysOf("Go back")).toEqual(["⌘", "["]);
    expect(keysOf("Go forward")).toEqual(["⌘", "]"]);
    expect(keysOf("New session")).toEqual(["⌘", "K"]);
  });
});
