import { useCallback, useEffect, useRef, useState } from "react";
import type { ToastMessage } from "@aidex/ui/components";

export type Notify = (message: string, tone?: ToastMessage["tone"]) => void;

const TOAST_DURATION_MS = 4000;
const MAX_VISIBLE = 3;

/** Local transient notifications for preview actions; render the result with `ToastRegion`. */
export function useToasts() {
  const [toasts, setToasts] = useState<readonly ToastMessage[]>([]);
  const timers = useRef(new Map<string, ReturnType<typeof setTimeout>>());

  const dismiss = useCallback((id: string) => {
    clearTimeout(timers.current.get(id));
    timers.current.delete(id);
    setToasts((current) => current.filter((toast) => toast.id !== id));
  }, []);

  const notify = useCallback<Notify>(
    (message, tone = "info") => {
      const id = `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
      setToasts((current) => [...current.slice(-(MAX_VISIBLE - 1)), { id, message, tone }]);
      timers.current.set(id, setTimeout(() => dismiss(id), TOAST_DURATION_MS));
    },
    [dismiss],
  );

  useEffect(() => {
    const pending = timers.current;
    return () => pending.forEach((timer) => clearTimeout(timer));
  }, []);

  return { toasts, notify, dismiss };
}
