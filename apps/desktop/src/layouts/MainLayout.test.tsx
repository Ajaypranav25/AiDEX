import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { NavigationProvider, useNavigation } from "../app/providers/navigation";
import { isApplePlatform } from "../lib/platform";
import { aidexConnection, aidexWrapper } from "../test/aidex";
import { MainLayout } from "./MainLayout";

vi.mock("../lib/platform", () => ({ isApplePlatform: vi.fn(() => false) }));

function OpenSettings() {
  const { navigate } = useNavigation();
  return <button onClick={() => navigate({ page: "settings" })}>Open settings</button>;
}

function renderLayout() {
  const Aidex = aidexWrapper(() => aidexConnection());
  return render(
    <Aidex>
      <NavigationProvider initialRoute={{ page: "chat" }}>
        <MainLayout Sidebar={() => null} onNewSession={() => {}}>
          <OpenSettings />
        </MainLayout>
      </NavigationProvider>
    </Aidex>,
  );
}

const button = (name: string) => screen.getByRole("button", { name });
const banner = () => screen.getByRole("banner");

describe("MainLayout history buttons", () => {
  it("walk back and forward through the visited screens and disable at either end", async () => {
    const user = userEvent.setup();
    renderLayout();
    expect(button("Back")).toBeDisabled();
    expect(button("Forward")).toBeDisabled();

    await user.click(button("Open settings"));
    expect(banner()).toHaveTextContent("Settings");
    expect(button("Back")).toBeEnabled();

    await user.click(button("Back"));
    expect(banner()).toHaveTextContent("Chat");
    expect(button("Back")).toBeDisabled();
    expect(button("Forward")).toBeEnabled();

    await user.click(button("Forward"));
    expect(banner()).toHaveTextContent("Settings");
    expect(button("Forward")).toBeDisabled();
  });

  it("name their shortcuts for each platform", () => {
    renderLayout();

    expect(button("Back")).toHaveAttribute("title", "Back (Alt+←)");
    expect(button("Back")).toHaveAttribute("aria-keyshortcuts", "Alt+ArrowLeft");
    expect(button("Forward")).toHaveAttribute("title", "Forward (Alt+→)");
    expect(button("Forward")).toHaveAttribute("aria-keyshortcuts", "Alt+ArrowRight");
  });

  it("name the ⌘ shortcuts on Apple platforms", () => {
    vi.mocked(isApplePlatform).mockReturnValue(true);

    renderLayout();

    expect(button("Back")).toHaveAttribute("title", "Back (⌘[)");
    expect(button("Back")).toHaveAttribute("aria-keyshortcuts", "Meta+[");
    expect(button("Forward")).toHaveAttribute("title", "Forward (⌘])");
    expect(button("Forward")).toHaveAttribute("aria-keyshortcuts", "Meta+]");
  });
});
