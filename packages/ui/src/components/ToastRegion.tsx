import { CheckCircleIcon, InfoIcon, XCircleIcon, XIcon } from "../icons";
import { IconButton } from "../primitives/IconButton";
import { cx } from "../utils/cx";
import styles from "./ToastRegion.module.css";

export type ToastMessage = { id: string; message: string; tone?: "info" | "success" | "danger" };

export type ToastRegionProps = {
  toasts: readonly ToastMessage[];
  onDismiss: (id: string) => void;
  dismissLabel?: string;
};

const ICON_BY_TONE = { info: InfoIcon, success: CheckCircleIcon, danger: XCircleIcon } as const;

/** Polite live region for transient notifications; it stays mounted so additions are announced. */
export function ToastRegion({ toasts, onDismiss, dismissLabel = "Dismiss notification" }: ToastRegionProps) {
  return (
    <div className={styles.region} role="status" aria-live="polite">
      {toasts.map(({ id, message, tone = "info" }) => {
        const Icon = ICON_BY_TONE[tone];
        return (
          <div key={id} className={cx(styles.toast, styles[tone])}>
            <Icon className={styles.icon} />
            <span className={styles.message}>{message}</span>
            <IconButton size="sm" label={dismissLabel} icon={<XIcon />} onClick={() => onDismiss(id)} />
          </div>
        );
      })}
    </div>
  );
}
