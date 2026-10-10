"use client";

import { FileDiff, type FileDiffMetadata, type FileDiffOptions } from "@pierre/diffs/react";
import type { CodeDiffLayout } from "./CodeDiff";
import styles from "./CodeDiff.module.css";

export type CodeDiffRendererProps = {
  fileDiff: FileDiffMetadata;
  layout: CodeDiffLayout;
};

/*
 * Rendering options shared by every diff:
 * - The JavaScript regex engine (`shiki-js`) keeps Shiki off WebAssembly, so the Tauri CSP needs no `wasm-unsafe-eval`.
 * - Themes and grammars load from bundled chunks; nothing is fetched at runtime.
 * - Our header replaces the library's, and long lines wrap so no inner scroller needs keyboard focus.
 * - Classic +/- indicators keep additions and deletions distinguishable without color.
 * - Hunk separators show line ranges; the library's expand controls are mouse-only, so they stay off.
 */
const BASE_OPTIONS = {
  theme: "pierre-dark",
  themeType: "dark",
  preferredHighlighter: "shiki-js",
  disableFileHeader: true,
  overflow: "wrap",
  diffIndicators: "classic",
  hunkSeparators: "metadata",
  lineDiffType: "word-alt",
} as const satisfies FileDiffOptions<undefined, undefined>;

/** Syntax-highlighted diff body. Loaded lazily by `CodeDiff`, and only in the browser. */
export default function CodeDiffRenderer({ fileDiff, layout }: CodeDiffRendererProps) {
  return <FileDiff fileDiff={fileDiff} options={{ ...BASE_OPTIONS, diffStyle: layout }} className={styles.surface} />;
}
