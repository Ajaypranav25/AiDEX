import type { ReactNode } from "react";
import { cx } from "../utils/cx";
import styles from "./SidebarItem.module.css";

export type SidebarItemProps = {
  label: string;
  icon?: ReactNode;
  /** Fixed icon color, e.g. a workspace's identity color. Active items otherwise use the accent. */
  iconTone?: "accent" | "success";
  active?: boolean;
  /** `sm` is a compact, icon-free row for long lists such as recents. */
  size?: "md" | "sm";
  onSelect?: () => void;
  /** Trailing count, e.g. items in a filter. */
  count?: number;
  /** Row actions (e.g. an overflow `IconButton`), revealed on hover, focus, or when active. */
  actions?: ReactNode;
};

export function SidebarItem({
  label,
  icon,
  iconTone,
  active = false,
  size = "md",
  onSelect,
  count,
  actions,
}: SidebarItemProps) {
  return (
    <li className={cx(styles.item, styles[size], active && styles.active, actions != null && styles.hasActions)}>
      <button type="button" className={styles.button} aria-current={active ? "true" : undefined} onClick={onSelect}>
        {icon ? <span className={cx(styles.icon, iconTone && styles[iconTone])}>{icon}</span> : null}
        <span className={styles.label}>{label}</span>
        {count !== undefined ? <span className={styles.count}>{count}</span> : null}
      </button>
      {actions != null ? <div className={styles.actions}>{actions}</div> : null}
    </li>
  );
}
