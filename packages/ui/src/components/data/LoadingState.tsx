import type { ComponentPropsWithRef, ReactNode } from "react";
import { LoaderIcon } from "../../icons/interface";
import { cx } from "../../utils/cx";
import styles from "./LoadingState.module.css";

export type LoadingStateProps = Omit<ComponentPropsWithRef<"div">, "children"> & {
  /** What is on its way, e.g. "Loading workspaces…". Announced politely to assistive technology. */
  label: ReactNode;
};

/** Inline placeholder while content loads: a spinner and what it waits for. Sized like a `Notice`. */
export function LoadingState({ label, className, ...props }: LoadingStateProps) {
  return (
    <div role="status" className={cx(styles.loading, className)} {...props}>
      <LoaderIcon className={styles.spinner} />
      <span>{label}</span>
    </div>
  );
}
