import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ErrorKind } from "@aidex/client/errors";
import { call } from "@aidex/client/rpc";
import { NavigationProvider } from "../../app/providers/navigation";
import { ToastProvider } from "../../app/providers/toast";
import { aidexConnection, aidexWrapper } from "../../test/aidex";
import { ProvidersPanel } from "./ProvidersPanel";

vi.mock("@aidex/client/rpc", () => ({ call: vi.fn() }));

const daemonDown = { code: "unavailable", kind: ErrorKind.daemonUnavailable, message: "AiDEX Core is not running." };

/** The daemon answers every method the panel reads, failing `failing` the first time it is called. */
function daemonFailsOnce(failing: "model.providers" | "provider.presets") {
  let failed = false;
  vi.mocked(call).mockImplementation(async (method) => {
    if (method === failing && !failed) {
      failed = true;
      throw daemonDown;
    }
    if (method === "model.providers") return { providers: [] };
    if (method === "provider.presets") return { presets: [] };
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
  it.each([
    ["model.providers", "Cannot list provider accounts"],
    ["provider.presets", "Cannot load provider presets"],
  ] as const)("explains when %s fails and reads it again on request", async (method, failure) => {
    const user = userEvent.setup();
    daemonFailsOnce(method);
    renderPanel();
    expect(await screen.findByText(`${failure}: AiDEX Core is not running.`)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Try again" }));

    await waitFor(() => expect(screen.queryByText(new RegExp(failure))).not.toBeInTheDocument());
    expect(vi.mocked(call).mock.calls.filter(([name]) => name === method)).toHaveLength(2);
  });
});
