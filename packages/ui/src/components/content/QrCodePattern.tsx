import type { ComponentPropsWithRef } from "react";
import { cx } from "../../utils/cx";
import styles from "./QrCodePattern.module.css";

export type QrCodePatternProps = Omit<ComponentPropsWithRef<"svg">, "children"> & {
  /** The same seed always draws the same pattern. */
  seed: string;
  /** Rendered width and height. */
  size?: number | string;
  /** Modules per side, clamped to the real QR range of 21 to 177. */
  modules?: number;
};

const FINDER = 7;
const QUIET_ZONE = 2;
const MIN_MODULES = 21;
const MAX_MODULES = 177;
const DEFAULT_MODULES = 25;

// FNV-1a; the xorshift generator below needs a non-zero state, hence the `|| 1`.
function hashSeed(seed: string): number {
  let hash = 0x811c9dc5;
  for (let index = 0; index < seed.length; index += 1) {
    hash = Math.imul(hash ^ seed.charCodeAt(index), 0x01000193);
  }
  return hash >>> 0 || 1;
}

function createCoinFlip(seed: string): () => boolean {
  let state = hashSeed(seed);
  return () => {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    state >>>= 0;
    return state >>> 31 === 1;
  };
}

// A module inside one of the three finder zones (7x7 finder plus a light separator), or null outside them.
function finderModule(row: number, col: number, count: number): boolean | null {
  const top = row <= FINDER;
  const left = col <= FINDER;
  const inZone = (top && left) || (top && col >= count - FINDER - 1) || (left && row >= count - FINDER - 1);
  if (!inZone) return null;
  const y = top ? row : row - (count - FINDER);
  const x = left ? col : col - (count - FINDER);
  if (x < 0 || y < 0 || x >= FINDER || y >= FINDER) return false;
  const ring = x === 0 || y === 0 || x === FINDER - 1 || y === FINDER - 1;
  const core = x >= 2 && x <= 4 && y >= 2 && y <= 4;
  return ring || core;
}

function buildRows(seed: string, count: number): boolean[][] {
  const flip = createCoinFlip(seed);
  return Array.from({ length: count }, (_, row) =>
    Array.from({ length: count }, (_, col) => {
      const finder = finderModule(row, col, count);
      if (finder !== null) return finder;
      if (row === FINDER - 1) return col % 2 === 0;
      if (col === FINDER - 1) return row % 2 === 0;
      return flip();
    }),
  );
}

// One path for every dark module, merging horizontal runs to keep it short.
function toPath(rows: boolean[][]): string {
  const segments: string[] = [];
  rows.forEach((cells, row) => {
    let start = -1;
    for (let col = 0; col <= cells.length; col += 1) {
      const dark = col < cells.length && cells[col];
      if (dark && start < 0) start = col;
      if (!dark && start >= 0) {
        const run = col - start;
        segments.push(`M${start + QUIET_ZONE} ${row + QUIET_ZONE}h${run}v1h-${run}z`);
        start = -1;
      }
    }
  });
  return segments.join("");
}

function moduleCount(modules: number): number {
  if (!Number.isFinite(modules)) return DEFAULT_MODULES;
  return Math.min(MAX_MODULES, Math.max(MIN_MODULES, Math.floor(modules)));
}

/** Decorative QR-style square drawn from a seed; a visual stand-in only, it does not encode any data and cannot be scanned. */
export function QrCodePattern({ seed, size = 120, modules = DEFAULT_MODULES, className, ...props }: QrCodePatternProps) {
  const count = moduleCount(modules);
  const extent = count + QUIET_ZONE * 2;
  return (
    <svg
      width={size}
      height={size}
      viewBox={`0 0 ${extent} ${extent}`}
      shapeRendering="crispEdges"
      aria-hidden="true"
      focusable="false"
      className={cx(styles.pattern, className)}
      {...props}
    >
      <rect className={styles.tile} width={extent} height={extent} rx={1} />
      <path className={styles.modules} d={toPath(buildRows(seed, count))} />
    </svg>
  );
}
