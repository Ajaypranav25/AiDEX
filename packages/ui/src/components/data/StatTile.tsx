import type { ComponentPropsWithRef, ReactNode } from "react";
import { Card } from "../../primitives/Card";
import { cx } from "../../utils/cx";
import styles from "./StatTile.module.css";

export type StatTileTone = "default" | "accent" | "success" | "warning" | "danger";

export type StatTileProps = Omit<ComponentPropsWithRef<"div">, "children"> & {
  /** What is measured, in sentence case, e.g. "Tokens per second". */
  label: ReactNode;
  /** The pre-formatted figure, e.g. "42.1" or "12.9K". */
  value: ReactNode;
  /** Context under the value, e.g. "Last 24 hours". State any status here too; tone is only an accent. */
  hint?: ReactNode;
  /** Decorative icon shown in a tinted chip. */
  icon?: ReactNode;
  /** Colors the icon chip, or a small marker beside the label when there is no icon. */
  tone?: StatTileTone;
};

/** Dashboard metric tile: a label, a headline value, and an optional hint. */
export function StatTile({ label, value, hint, icon, tone = "default", className, ...props }: StatTileProps) {
  const showMarker = !icon && tone !== "default";
  return (
    <Card padding="md" className={cx(styles.tile, styles[tone], className)} {...props}>
      <dl className={styles.body}>
        <dt className={cx(styles.label, showMarker && styles.marker)}>{label}</dt>
        <dd className={styles.value}>{value}</dd>
        {hint ? <dd className={styles.hint}>{hint}</dd> : null}
      </dl>
      {icon ? (
        <span className={styles.icon} aria-hidden="true">
          {icon}
        </span>
      ) : null}
    </Card>
  );
}
