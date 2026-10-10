"use client";

import type { ComponentPropsWithRef } from "react";
import { cx } from "../../utils/cx";
import styles from "./Timeline.module.css";

export type TimelineItem = {
  id: string;
  title: string;
  /** Display text, e.g. "Today, 10:24 AM". */
  time: string;
  description?: string;
};

export type TimelineProps = Omit<ComponentPropsWithRef<"ol">, "children" | "onSelect"> & {
  items: readonly TimelineItem[];
  selectedId?: string;
  onSelect: (id: string) => void;
};

/** Selectable vertical list of points in time, such as checkpoints; the selected item is marked `aria-current`. */
export function Timeline({ items, selectedId, onSelect, className, ...props }: TimelineProps) {
  return (
    <ol className={cx(styles.list, className)} {...props}>
      {items.map((item) => {
        const isSelected = item.id === selectedId;
        return (
          <li key={item.id} className={styles.entry}>
            <button
              type="button"
              className={cx(styles.item, isSelected && styles.selected)}
              aria-current={isSelected ? "true" : undefined}
              onClick={() => onSelect(item.id)}
            >
              <span className={styles.title}>{item.title}</span>
              <span className={styles.time}>{item.time}</span>
              {item.description ? <span className={styles.description}>{item.description}</span> : null}
            </button>
          </li>
        );
      })}
    </ol>
  );
}
