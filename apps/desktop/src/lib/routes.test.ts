import { describe, expect, it } from "vitest";
import { railIdFor } from "./navigation";
import { screenKeyOf, tabOf } from "./routes";

describe("tabOf", () => {
  it("reads a known tab and falls back to the first tab otherwise", () => {
    expect(tabOf({ page: "settings", tab: "drive" }, "settings")).toBe("drive");
    expect(tabOf({ page: "settings", tab: "no-such-tab" }, "settings")).toBe("appearance");
    expect(tabOf({ page: "settings" }, "settings")).toBe("appearance");
  });
});

describe("screenKeyOf", () => {
  it("gives each record its own screen", () => {
    expect(screenKeyOf({ page: "workspace", id: "w1" })).not.toBe(screenKeyOf({ page: "workspace", id: "w2" }));
  });

  it("keeps one screen across tab switches and in-page selections", () => {
    expect(screenKeyOf({ page: "workspace", id: "w1", tab: "git" })).toBe(screenKeyOf({ page: "workspace", id: "w1" }));
    expect(screenKeyOf({ page: "review", id: "src/main.rs" })).toBe(screenKeyOf({ page: "review" }));
  });
});

describe("railIdFor", () => {
  it.each([
    [{ page: "workspace-create" }, "workspaces"],
    [{ page: "workspace", id: "w1" }, "workspaces"],
    [{ page: "execution" }, "agents"],
    [{ page: "agent-config", id: "planner" }, "agents"],
    [{ page: "checkpoints" }, "history"],
    [{ page: "model", id: "qwen" }, "models"],
    [{ page: "models", tab: "providers" }, "providers"],
    [{ page: "models", tab: "local" }, "models"],
    [{ page: "settings" }, "settings"],
  ] as const)("highlights the rail item for %o", (route, railId) => {
    expect(railIdFor(route)).toBe(railId);
  });
});
