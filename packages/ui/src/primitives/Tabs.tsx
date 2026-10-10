"use client";

import { useRef, type KeyboardEvent, type ReactNode } from "react";
import { cx } from "../utils/cx";
import styles from "./Tabs.module.css";

export type TabItem<Value extends string> = { value: Value; label: string; icon?: ReactNode; count?: number };

export type TabsProps<Value extends string> = {
  /** Base id linking each tab to its `TabPanel`. */
  id: string;
  /** Accessible name for the tab list. */
  label: string;
  items: readonly TabItem<Value>[];
  value: Value;
  onValueChange: (value: Value) => void;
  /** `pill` for in-page view switches; `underline` for top-level page sections. */
  variant?: "pill" | "underline";
  className?: string;
};

const STEP_BY_KEY: Partial<Record<string, number>> = { ArrowRight: 1, ArrowLeft: -1 };

export const tabId = (tabsId: string, value: string) => `${tabsId}-tab-${value}`;
export const tabPanelId = (tabsId: string, value: string) => `${tabsId}-panel-${value}`;

/** Tab list with roving focus (arrow keys, Home, End) and automatic activation. */
export function Tabs<Value extends string>({
  id,
  label,
  items,
  value,
  onValueChange,
  variant = "pill",
  className,
}: TabsProps<Value>) {
  const tabRefs = useRef<Array<HTMLButtonElement | null>>([]);

  const selectAt = (index: number) => {
    const wrapped = (index + items.length) % items.length;
    onValueChange(items[wrapped].value);
    tabRefs.current[wrapped]?.focus();
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    const step = STEP_BY_KEY[event.key];
    if (step !== undefined) selectAt(index + step);
    else if (event.key === "Home") selectAt(0);
    else if (event.key === "End") selectAt(items.length - 1);
    else return;
    event.preventDefault();
  };

  return (
    <div role="tablist" aria-label={label} className={cx(styles.list, styles[variant], className)}>
      {items.map((item, index) => {
        const selected = item.value === value;
        return (
          <button
            key={item.value}
            ref={(node) => {
              tabRefs.current[index] = node;
            }}
            type="button"
            role="tab"
            id={tabId(id, item.value)}
            aria-selected={selected}
            aria-controls={selected ? tabPanelId(id, item.value) : undefined}
            tabIndex={selected ? 0 : -1}
            className={cx(styles.tab, selected && styles.selected)}
            onClick={() => onValueChange(item.value)}
            onKeyDown={(event) => handleKeyDown(event, index)}
          >
            {item.icon ? <span className={styles.icon}>{item.icon}</span> : null}
            {item.label}
            {item.count !== undefined ? <span className={styles.count}>{item.count}</span> : null}
          </button>
        );
      })}
    </div>
  );
}

export type TabPanelProps = {
  /** The `id` passed to the matching `Tabs`. */
  tabsId: string;
  /** Value of the tab this panel belongs to; render only the selected panel. */
  value: string;
  children: ReactNode;
  className?: string;
};

export function TabPanel({ tabsId, value, children, className }: TabPanelProps) {
  return (
    <div
      role="tabpanel"
      id={tabPanelId(tabsId, value)}
      aria-labelledby={tabId(tabsId, value)}
      tabIndex={0}
      className={cx(styles.panel, className)}
    >
      {children}
    </div>
  );
}
