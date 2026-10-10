"use client";

import { parseDiffFromFile, parsePatchFiles, type FileDiffMetadata } from "@pierre/diffs";
import {
  Component,
  lazy,
  Suspense,
  useId,
  useMemo,
  useState,
  useSyncExternalStore,
  type ComponentPropsWithRef,
  type CSSProperties,
  type ReactNode,
} from "react";
import { FileCodeIcon } from "../../icons";
import { SegmentedControl } from "../../primitives/SegmentedControl";
import { cx } from "../../utils/cx";
import a11y from "./a11y.module.css";
import styles from "./CodeDiff.module.css";

export type CodeDiffLayout = "unified" | "split";

/** Either both file versions or a unified patch (`diff -u` / `git diff` output for one file). */
export type CodeDiffSource =
  | { oldCode: string; newCode: string; patch?: never }
  | { patch: string; oldCode?: never; newCode?: never };

export type CodeDiffProps = Omit<ComponentPropsWithRef<"section">, "children"> &
  CodeDiffSource & {
    /** Path shown in the header; its extension picks the highlighter grammar unless `language` is set. */
    fileName: string;
    /** Shiki language id such as `ts`, `tsx`, `json`, or `rust`. */
    language?: string;
    /** Controlled layout. Omit to let the built-in toggle manage it. */
    layout?: CodeDiffLayout;
    defaultLayout?: CodeDiffLayout;
    onLayoutChange?: (layout: CodeDiffLayout) => void;
    /** Shows the unified/split toggle in the header. Defaults to true. */
    showLayoutToggle?: boolean;
    /** Header controls, e.g. accept and reject buttons. */
    actions?: ReactNode;
  };

const LAYOUT_OPTIONS = [
  { value: "unified", label: "Unified" },
  { value: "split", label: "Split" },
] as const;

/** Approximate rendered row height, used to reserve space while the highlighter loads. */
const ROW_HEIGHT_PX = 20;

const CodeDiffRenderer = lazy(() => import("./CodeDiffRenderer"));

function countLabel(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

function parseSource(fileName: string, language: string | undefined, source: CodeDiffSource): FileDiffMetadata | null {
  if (source.patch !== undefined) {
    const parsed = parsePatchFiles(source.patch)[0]?.files[0];
    return parsed ? { ...parsed, lang: language ?? parsed.lang } : null;
  }
  return parseDiffFromFile(
    { name: fileName, contents: source.oldCode, lang: language },
    { name: fileName, contents: source.newCode, lang: language },
  );
}

export type DiffStats = { additions: number; deletions: number };

function countChanges(fileDiff: FileDiffMetadata | null): DiffStats {
  return (fileDiff?.hunks ?? []).reduce(
    (sum, hunk) => ({ additions: sum.additions + hunk.additionLines, deletions: sum.deletions + hunk.deletionLines }),
    { additions: 0, deletions: 0 },
  );
}

/** Added and deleted line counts for a diff source, matching what `CodeDiff` shows in its header. */
export function getDiffStats(fileName: string, source: CodeDiffSource): DiffStats {
  return countChanges(parseSource(fileName, undefined, source));
}

const subscribeToNothing = () => () => {};

/** False during prerender and hydration, true afterwards. The highlighter touches browser-only APIs. */
function useIsClient(): boolean {
  return useSyncExternalStore(
    subscribeToNothing,
    () => true,
    () => false,
  );
}

type BoundaryProps = { fallback: ReactNode; children: ReactNode };

/** Shows `fallback` if the highlighter chunk fails to load or throws while rendering. */
class RenderBoundary extends Component<BoundaryProps, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(error: unknown) {
    console.error("CodeDiff: failed to render diff", error);
  }

  render() {
    return this.state.failed ? this.props.fallback : this.props.children;
  }
}

/**
 * Syntax-highlighted file diff built on `@pierre/diffs`, with an accessible header (file name, change counts,
 * layout toggle, actions). The highlighter loads lazily in the browser; prerendered HTML holds the header and a
 * placeholder sized to the diff.
 */
export function CodeDiff({
  fileName,
  language,
  oldCode,
  newCode,
  patch,
  layout,
  defaultLayout = "unified",
  onLayoutChange,
  showLayoutToggle = true,
  actions,
  className,
  ...props
}: CodeDiffProps) {
  const isClient = useIsClient();
  const statsId = useId();
  const [uncontrolledLayout, setUncontrolledLayout] = useState<CodeDiffLayout>(defaultLayout);
  const activeLayout = layout ?? uncontrolledLayout;

  const fileDiff = useMemo(
    () =>
      parseSource(
        fileName,
        language,
        patch !== undefined ? { patch } : { oldCode: oldCode ?? "", newCode: newCode ?? "" },
      ),
    [fileName, language, oldCode, newCode, patch],
  );
  const { additions, deletions } = countChanges(fileDiff);
  const hasChanges = additions + deletions > 0;
  const showToggle = showLayoutToggle && hasChanges;

  const changeLayout = (next: CodeDiffLayout) => {
    if (layout === undefined) setUncontrolledLayout(next);
    onLayoutChange?.(next);
  };

  const rowCount = fileDiff ? (activeLayout === "split" ? fileDiff.splitLineCount : fileDiff.unifiedLineCount) : 0;
  const placeholderStyle = { "--code-diff-placeholder-height": `${rowCount * ROW_HEIGHT_PX}px` } as CSSProperties;
  const placeholder = (
    <div className={styles.placeholder} style={placeholderStyle}>
      <span className={styles.status}>Loading diff…</span>
    </div>
  );

  let body: ReactNode;
  if (!fileDiff) body = <p className={styles.status}>This patch could not be read.</p>;
  else if (!hasChanges) body = <p className={styles.status}>No changes.</p>;
  else if (!isClient) body = placeholder;
  else
    body = (
      <RenderBoundary key={fileName} fallback={<p className={styles.status}>This diff could not be displayed.</p>}>
        <Suspense fallback={placeholder}>
          <CodeDiffRenderer fileDiff={fileDiff} layout={activeLayout} />
        </Suspense>
      </RenderBoundary>
    );

  return (
    <section aria-label={`Changes to ${fileName}`} aria-describedby={statsId} className={cx(styles.diff, className)} {...props}>
      <header className={styles.header}>
        <span className={styles.fileIcon} aria-hidden="true">
          <FileCodeIcon />
        </span>
        <span className={styles.fileName} title={fileName}>
          {fileName}
        </span>
        <span className={styles.stats}>
          <span className={styles.added} aria-hidden="true">
            +{additions}
          </span>
          <span className={styles.removed} aria-hidden="true">
            −{deletions}
          </span>
          <span id={statsId} className={a11y.visuallyHidden}>
            {countLabel(additions, "addition")}, {countLabel(deletions, "deletion")}
          </span>
        </span>
        {showToggle || actions ? (
          <div className={styles.controls}>
            {showToggle ? (
              <SegmentedControl
                label={`Diff layout for ${fileName}`}
                size="sm"
                options={LAYOUT_OPTIONS}
                value={activeLayout}
                onValueChange={changeLayout}
              />
            ) : null}
            {actions ? <div className={styles.actions}>{actions}</div> : null}
          </div>
        ) : null}
      </header>
      <div className={styles.body}>{body}</div>
    </section>
  );
}
