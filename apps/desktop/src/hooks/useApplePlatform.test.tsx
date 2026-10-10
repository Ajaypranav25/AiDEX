import { renderHook } from "@testing-library/react";
import { renderToString } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { isApplePlatform } from "../lib/platform";
import { useApplePlatform } from "./useApplePlatform";

vi.mock("../lib/platform", () => ({ isApplePlatform: vi.fn(() => false) }));

function Probe() {
  return <span>{useApplePlatform() ? "apple" : "other"}</span>;
}

describe("useApplePlatform", () => {
  it("assumes another platform while prerendering, whatever the browser reports", () => {
    vi.mocked(isApplePlatform).mockReturnValue(true);

    expect(renderToString(<Probe />)).toContain("other");
  });

  it("reads the platform in the browser", () => {
    vi.mocked(isApplePlatform).mockReturnValue(true);

    const { result } = renderHook(() => useApplePlatform());

    expect(result.current).toBe(true);
  });
});
