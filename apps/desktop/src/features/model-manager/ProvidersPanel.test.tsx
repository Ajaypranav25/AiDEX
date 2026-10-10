import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ProviderPreset } from "@aidex/client/dto/models";
import { ErrorKind } from "@aidex/client/errors";
import { call } from "@aidex/client/rpc";
import { NavigationProvider } from "../../app/providers/navigation";
import { ToastProvider } from "../../app/providers/toast";
import { aidexConnection, aidexWrapper } from "../../test/aidex";
import { ProvidersPanel } from "./ProvidersPanel";

vi.mock("@aidex/client/rpc", () => ({ call: vi.fn() }));

const daemonDown = { code: "unavailable", kind: ErrorKind.daemonUnavailable, message: "AiDEX Core is not running." };

const openai = { id: "openai", display_name: "OpenAI", base_url: "https://api.openai.com/v1" } as ProviderPreset;

/** The daemon answers every method the panel reads, failing `failing` the first time it is called. */
function daemonFailsOnce(failing: "model.providers" | "provider.presets") {
  let failed = false;
  vi.mocked(call).mockImplementation(async (method) => {
    if (method === failing && !failed) {
      failed = true;
      throw daemonDown;
    }
    if (method === "model.providers") return { providers: [] };
    if (method === "provider.presets") return { presets: [openai] };
    if (method === "model.list") return { models: [], failures: [], default_model: null };
    throw new Error(`unexpected method ${method}`);
  });
}

function renderPanel() {
  const Aidex = aidexWrapper(() => aidexConnection());
  return render(
    <Aidex>
      <NavigationProvider initialRoute={{ page: "models", tab: "providers" }}>
        <ToastProvider>
          <ProvidersPanel />
        </ToastProvider>
      </NavigationProvider>
    </Aidex>,
  );
}

describe("ProvidersPanel", () => {
  it("lists the accounts after a failed read is retried", async () => {
    const user = userEvent.setup();
    daemonFailsOnce("model.providers");
    renderPanel();
    expect(await screen.findByRole("alert")).toHaveTextContent("Cannot list provider accounts: AiDEX Core is not running.");

    await user.click(screen.getByRole("button", { name: "Try again: Cannot list provider accounts" }));

    expect(await screen.findByText("No accounts connected yet.")).toBeInTheDocument();
  });

  it("offers the providers after a failed preset read is retried", async () => {
    const user = userEvent.setup();
    daemonFailsOnce("provider.presets");
    renderPanel();
    expect(await screen.findByRole("alert")).toHaveTextContent("Cannot load provider presets: AiDEX Core is not running.");

    await user.click(screen.getByRole("button", { name: "Try again: Cannot load provider presets" }));

    expect(await screen.findByRole("option", { name: "OpenAI" })).toBeInTheDocument();
  });
});
