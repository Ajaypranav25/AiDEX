"use client";

import { useState, type ComponentPropsWithRef, type CSSProperties, type ReactNode } from "react";
import { ChevronRightIcon, FileIcon, FolderIcon } from "../../icons";
import { cx } from "../../utils/cx";
import styles from "./FileTree.module.css";

export type TreeNode = {
  id: string;
  name: string;
  kind: "folder" | "file";
  children?: readonly TreeNode[];
  /** Trailing detail such as a change count or status letter. */
  meta?: string;
  /** Replaces the default folder or file icon. */
  icon?: ReactNode;
};

export type FileTreeProps = Omit<ComponentPropsWithRef<"ul">, "children" | "onSelect"> & {
  nodes: readonly TreeNode[];
  selectedId?: string;
  /** Called with a file's id; without it, files render as plain, non-interactive rows. */
  onSelect?: (id: string) => void;
  /** Folder ids that start expanded. */
  defaultExpanded?: readonly string[];
};

type ItemProps = {
  node: TreeNode;
  depth: number;
  expanded: ReadonlySet<string>;
  onToggle: (id: string) => void;
  selectedId?: string;
  onSelect?: (id: string) => void;
};

function RowContent({ node }: { node: TreeNode }) {
  const isFolder = node.kind === "folder";
  return (
    <>
      <span className={styles.chevron} aria-hidden="true">
        {isFolder ? <ChevronRightIcon /> : null}
      </span>
      <span className={styles.icon} aria-hidden="true">
        {node.icon ?? (isFolder ? <FolderIcon /> : <FileIcon />)}
      </span>
      <span className={styles.name}>{node.name}</span>
      {node.meta ? <span className={styles.meta}>{node.meta}</span> : null}
    </>
  );
}

function TreeItem({ node, depth, ...context }: ItemProps) {
  const { expanded, onToggle, selectedId, onSelect } = context;
  const indent = { "--file-tree-depth": depth } as CSSProperties;

  if (node.kind === "folder") {
    const isOpen = expanded.has(node.id);
    return (
      <li>
        <button type="button" className={styles.row} style={indent} aria-expanded={isOpen} onClick={() => onToggle(node.id)}>
          <RowContent node={node} />
        </button>
        {isOpen && node.children?.length ? (
          <ul className={styles.group}>
            {node.children.map((child) => (
              <TreeItem key={child.id} node={child} depth={depth + 1} {...context} />
            ))}
          </ul>
        ) : null}
      </li>
    );
  }

  const isSelected = node.id === selectedId;
  const current = isSelected ? "true" : undefined;
  return (
    <li>
      {onSelect ? (
        <button
          type="button"
          className={cx(styles.row, isSelected && styles.selected)}
          style={indent}
          aria-current={current}
          onClick={() => onSelect(node.id)}
        >
          <RowContent node={node} />
        </button>
      ) : (
        <div className={cx(styles.row, styles.static, isSelected && styles.selected)} style={indent} aria-current={current}>
          <RowContent node={node} />
        </div>
      )}
    </li>
  );
}

/** Collapsible folder and file list; folders toggle with a chevron, files are selectable. */
export function FileTree({ nodes, selectedId, onSelect, defaultExpanded, className, ...props }: FileTreeProps) {
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => new Set(defaultExpanded));

  const toggle = (id: string) =>
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  return (
    <ul aria-label="Files" className={cx(styles.tree, className)} {...props}>
      {nodes.map((node) => (
        <TreeItem
          key={node.id}
          node={node}
          depth={0}
          expanded={expanded}
          onToggle={toggle}
          selectedId={selectedId}
          onSelect={onSelect}
        />
      ))}
    </ul>
  );
}
