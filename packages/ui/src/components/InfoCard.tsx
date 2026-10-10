import type { ReactNode } from "react";
import { Card } from "../primitives/Card";
import { cx } from "../utils/cx";
import styles from "./InfoCard.module.css";

export type InfoCardProps = {
  /** Decorative icon shown top-left. */
  icon: ReactNode;
  /** Status shown top-right, typically a `Badge`. */
  status?: ReactNode;
  /** What is being described, e.g. "CPU". */
  label: string;
  /** The headline fact, e.g. the detected model. */
  value: ReactNode;
  /** Supporting detail under the value. */
  detail?: ReactNode;
  className?: string;
};

/** Card summarizing one fact: label, value, and detail, with an icon and status. */
export function InfoCard({ icon, status, label, value, detail, className }: InfoCardProps) {
  // Status follows the facts in DOM order so assistive technology reads "CPU ..." before "Detected";
  // the grid places it top-right visually.
  return (
    <Card className={cx(styles.card, className)}>
      <span className={styles.icon}>{icon}</span>
      <dl className={styles.body}>
        <dt className={styles.label}>{label}</dt>
        <dd className={styles.value}>{value}</dd>
        {detail ? <dd className={styles.detail}>{detail}</dd> : null}
      </dl>
      {status ? <span className={styles.status}>{status}</span> : null}
    </Card>
  );
}
