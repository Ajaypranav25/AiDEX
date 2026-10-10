"use client";

import { useRef, type ReactNode } from "react";
import { RefreshIcon } from "../../icons/interface";
import { Button } from "../../primitives/Button";
import { Notice } from "../Notice";
import { LoadingState } from "./LoadingState";
import styles from "./AsyncContent.module.css";

export type AsyncContentProps = {
  /** `loading` until the first answer, `failed` with a retry, or `ready` to show `children`. */
  state: "loading" | "failed" | "ready";
  /** What is on its way, e.g. "Loading workspaces…". */
  loadingLabel: ReactNode;
  /** Why it failed, e.g. "Cannot list workspaces: AiDEX Core is not running."; announced as an alert. */
  failure?: ReactNode;
  /** Asks again. Focus waits on the region meanwhile instead of leaving the page with the button. */
  onRetry?: () => void;
  /** Accessible name of the retry button, starting with "Try again", when several failures can show at once. */
  retryLabel?: string;
  children?: ReactNode;
};

/** Content that loads: a spinner, then the content, or a failure with a retry that keeps keyboard focus. */
export function AsyncContent({ state, loadingLabel, failure, onRetry, retryLabel, children }: AsyncContentProps) {
  const regionRef = useRef<HTMLDivElement>(null);

  const retry = () => {
    regionRef.current?.focus();
    onRetry?.();
  };

  return (
    <div ref={regionRef} tabIndex={-1} aria-busy={state === "loading"} className={styles.region}>
      {state === "loading" ? <LoadingState label={loadingLabel} /> : null}
      {state === "failed" ? (
        <Notice
          tone="warning"
          role="alert"
          action={
            onRetry ? (
              <Button variant="outline" size="md" icon={<RefreshIcon />} aria-label={retryLabel} onClick={retry}>
                Try again
              </Button>
            ) : undefined
          }
        >
          {failure}
        </Notice>
      ) : null}
      {state === "ready" ? children : null}
    </div>
  );
}
