import type { ComponentPropsWithRef, ReactNode } from "react";
import { CheckIcon, CircleIcon, LoaderIcon, XIcon } from "../../icons";
import { cx } from "../../utils/cx";
import a11y from "./a11y.module.css";
import styles from "./TaskGraph.module.css";

export type GraphNodeStatus = "done" | "running" | "pending" | "failed";

export type GraphNode = {
  id: string;
  label: string;
  /** Decorative leading icon. */
  icon?: ReactNode;
  status?: GraphNodeStatus;
  /** Secondary line, e.g. the file or tool the worker is on. */
  detail?: string;
};

export type TaskGraphProps = Omit<ComponentPropsWithRef<"ul">, "children"> & {
  /** The orchestrator, drawn with the accent border. */
  root: GraphNode;
  /** Workers the root delegates to. */
  nodes: readonly GraphNode[];
};

const STATUS_TEXT: Record<GraphNodeStatus, string> = {
  done: "Done",
  running: "Running",
  pending: "Pending",
  failed: "Failed",
};

const STATUS_ICON: Record<GraphNodeStatus, ReactNode> = {
  done: <CheckIcon />,
  running: <LoaderIcon />,
  pending: <CircleIcon />,
  failed: <XIcon />,
};

function NodeCard({ node, isRoot = false }: { node: GraphNode; isRoot?: boolean }) {
  return (
    <div className={cx(styles.node, isRoot && styles.rootNode, node.status && styles[node.status])}>
      {node.icon ? (
        <span className={styles.icon} aria-hidden="true">
          {node.icon}
        </span>
      ) : null}
      <span className={styles.text}>
        <span className={styles.label}>{node.label}</span>
        {node.detail ? <span className={styles.detail}>{node.detail}</span> : null}
      </span>
      {node.status ? (
        <span className={styles.status}>
          {STATUS_ICON[node.status]}
          <span className={a11y.visuallyHidden}>{STATUS_TEXT[node.status]}</span>
        </span>
      ) : null}
    </div>
  );
}

/** Orchestrator-to-workers diagram, built as a nested list so screen readers hear the root, then each worker with its status. */
export function TaskGraph({ root, nodes, className, ...props }: TaskGraphProps) {
  return (
    <ul aria-label="Task graph" className={cx(styles.graph, className)} {...props}>
      <li className={styles.tree}>
        <NodeCard node={root} isRoot />
        {nodes.length > 0 ? (
          <ul className={styles.children}>
            {nodes.map((node) => (
              <li key={node.id} className={styles.worker}>
                <NodeCard node={node} />
              </li>
            ))}
          </ul>
        ) : null}
      </li>
    </ul>
  );
}
