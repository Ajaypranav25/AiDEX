import { describe, expect, it } from "vitest";
import { UNKNOWN, bytesToGb, formatBytes, formatDuration } from "./format";

describe("formatBytes", () => {
  it.each([null, undefined, Number.NaN, Number.POSITIVE_INFINITY, -1])("renders %s as unknown", (value) => {
    expect(formatBytes(value)).toBe(UNKNOWN);
  });

  it.each([
    [0, "0 bytes"],
    [1, "1 byte"],
    [1023, "1023 bytes"],
    [1024, "1.0 KB"],
    [1536, "1.5 KB"],
    [100 * 1024, "100 KB"],
    [15_676_630_630, "14.6 GB"],
    [123_048_316_928, "115 GB"],
    [1024 ** 5, "1.0 PB"],
    [1024 ** 6, "1024 PB"],
  ])("formats %d bytes as %s", (bytes, text) => {
    expect(formatBytes(bytes)).toBe(text);
  });
});

describe("bytesToGb", () => {
  it("divides by 1024³", () => {
    expect(bytesToGb(3 * 1024 ** 3)).toBe(3);
  });
});

describe("formatDuration", () => {
  it.each([null, undefined, Number.NaN, -5])("renders %s as unknown", (value) => {
    expect(formatDuration(value)).toBe(UNKNOWN);
  });

  it.each([
    [0, "0 s"],
    [12_999, "12 s"],
    [250_000, "4 min 10 s"],
    [7_500_000, "2 h 5 min"],
  ])("formats %d ms as %s", (milliseconds, text) => {
    expect(formatDuration(milliseconds)).toBe(text);
  });
});
