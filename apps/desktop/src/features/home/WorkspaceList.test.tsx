import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Workspace } from "@aidex/client/dto/workspace";
import { ErrorKind } from "@aidex/client/errors";
import { call } from "@aidex/client/rpc";
import { aidexConnection, aidexWrapper } from "../../test/aidex";
import { WorkspaceList } from "./WorkspaceList";

vi.mock("@aidex/client/rpc", () => ({ call: vi.fn() }));

const workspace = (id: string, name: string, lastOpenedAt: string | null): Workspace => ({
  id,
  name,
  kind: { type: "host" },
  root: { type: "host", path: `C:\\src\\${name}` },
  created_at: "2026-10-01T09:00:00Z",
  last_opened_at: lastOpenedAt,
  path: `C:\\src\\${name}`,
  availability: { state: "available" },
  repository: { vcs: "git", root: `C:\\src\\${name}`, is_workspace_root: true },
});

const renderList = () => render(<WorkspaceList />, { wrapper: aidexWrapper(() => aidexConnection()) });

describe("WorkspaceList", () => {
  it("shows that workspaces are loading", () => {
    vi.mocked(call).mockReturnValue(new Promise(() => {}));

    renderList();

    expect(screen.getByRole("status")).toHaveTextContent("Loading workspaces…");
  });

  it("lists workspaces, most recently opened first", async () => {
    vi.mocked(call).mockResolvedValue({
      workspaces: [workspace("wsp_1", "api", "2026-10-02T09:00:00Z"), workspace("wsp_2", "web", "2026-10-09T09:00:00Z")],
      selected: "wsp_1",
    });

    renderList();

    const rows = await screen.findAllByRole("listitem");
    expect(rows.map((row) => row.textContent)).toEqual([
      expect.stringContaining("web"),
      expect.stringContaining("api"),
    ]);
    expect(rows[1]).toHaveTextContent("Selected");
  });

  it("announces a failure and lists the workspaces after a retry", async () => {
    const user = userEvent.setup();
    vi.mocked(call)
      .mockRejectedValueOnce({ code: "unavailable", kind: ErrorKind.daemonUnavailable, message: "AiDEX Core is not running." })
      .mockResolvedValueOnce({ workspaces: [workspace("wsp_1", "api", null)], selected: null });
    renderList();
    expect(await screen.findByRole("alert")).toHaveTextContent("Cannot list workspaces: AiDEX Core is not running.");

    await user.click(screen.getByRole("button", { name: "Try again: Cannot list workspaces" }));

    expect(await screen.findByText("api")).toBeInTheDocument();
    expect(call).toHaveBeenCalledTimes(2);
    expect(screen.queryByText(/Cannot list workspaces/)).not.toBeInTheDocument();
  });
});
