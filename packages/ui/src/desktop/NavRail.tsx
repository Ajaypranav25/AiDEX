import type { ReactNode } from "react";
import { IconButton } from "../primitives/IconButton";
import styles from "./NavRail.module.css";

export type NavRailItem<Id extends string> = { id: Id; label: string; icon: ReactNode };

export type NavRailProps<Id extends string> = {
  /** Accessible name for the navigation landmark. */
  label: string;
  items: readonly NavRailItem<Id>[];
  activeId: Id;
  onSelect: (id: Id) => void;
};

/** Vertical icon navigation; each destination is labelled for assistive technology and tooltips. */
export function NavRail<Id extends string>({ label, items, activeId, onSelect }: NavRailProps<Id>) {
  return (
    <nav aria-label={label} className={styles.rail}>
      <ul className={styles.list}>
        {items.map((item) => {
          const active = item.id === activeId;
          return (
            <li key={item.id}>
              <IconButton
                label={item.label}
                icon={item.icon}
                size="xl"
                selected={active}
                aria-current={active ? "page" : undefined}
                onClick={() => onSelect(item.id)}
              />
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
