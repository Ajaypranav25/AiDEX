import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { call } from "@aidex/client/rpc";
import { ErrorKind, toAidexError } from "@aidex/client/errors";
import type { ModelPreferences } from "@aidex/client/dto/preferences";
import { useToast } from "../../../app/providers/toast";
import { useRpc } from "../../../hooks/useRpc";

/** Bounds enforced by the daemon (`aidex-preferences::ModelPreferences`). */
const MAX_FAVORITES = 64;
const MAX_HIDDEN = 512;
const EMPTY: ModelPreferences = { favorite_models: [], hidden_models: [] };

export type ModelPreferencesState = {
  favorites: ReadonlySet<string>;
  hidden: ReadonlySet<string>;
  toggleFavorite: (key: string) => void;
  setHidden: (key: string, hidden: boolean) => void;
  /** Why favorites and hidden models cannot be read or saved right now, if they cannot. */
  error: string | null;
};

/**
 * Model picker favorites and hidden models (`model.preferences.*`). Changes apply at once and roll
 * back with a toast when the daemon refuses them; an older daemon without the methods reads as empty.
 */
export function useModelPreferences(): ModelPreferencesState {
  const notify = useToast();
  const query = useRpc("model.preferences.get");
  const [local, setLocal] = useState<ModelPreferences | null>(null);
  // A fresh answer from the daemon replaces optimistic state.
  useEffect(() => setLocal(null), [query.data]);
  const current = local ?? query.data ?? EMPTY;
  const currentRef = useRef(current);
  currentRef.current = current;
  const unsupported = query.error?.kind === ErrorKind.methodNotFound;

  const save = useCallback(
    (next: ModelPreferences, patch: Partial<ModelPreferences>) => {
      if (unsupported) {
        notify("Favorites and hidden models need a newer AiDEX Core.", "danger");
        return;
      }
      const previous = currentRef.current;
      currentRef.current = next;
      setLocal(next);
      call("model.preferences.set", patch).then(
        (saved) => {
          currentRef.current = saved;
          setLocal(saved);
        },
        (caught: unknown) => {
          currentRef.current = previous;
          setLocal(previous);
          notify(`Model preferences were not saved: ${toAidexError(caught).message}`, "danger");
        },
      );
    },
    [notify, unsupported],
  );

  const toggleFavorite = useCallback(
    (key: string) => {
      const before = currentRef.current;
      const has = before.favorite_models.includes(key);
      const favorite_models = has
        ? before.favorite_models.filter((entry) => entry !== key)
        : [...before.favorite_models, key];
      if (favorite_models.length > MAX_FAVORITES) {
        notify(`You can keep up to ${MAX_FAVORITES} favorite models.`, "danger");
        return;
      }
      save({ ...before, favorite_models }, { favorite_models });
    },
    [notify, save],
  );

  const setHidden = useCallback(
    (key: string, hide: boolean) => {
      const before = currentRef.current;
      if (before.hidden_models.includes(key) === hide) return;
      const hidden_models = hide
        ? [...before.hidden_models, key]
        : before.hidden_models.filter((entry) => entry !== key);
      if (hidden_models.length > MAX_HIDDEN) {
        notify(`You can hide up to ${MAX_HIDDEN} models.`, "danger");
        return;
      }
      save({ ...before, hidden_models }, { hidden_models });
    },
    [notify, save],
  );

  const favorites = useMemo(() => new Set(current.favorite_models), [current.favorite_models]);
  const hidden = useMemo(() => new Set(current.hidden_models), [current.hidden_models]);
  const error = unsupported ? null : (query.error?.message ?? null);
  return useMemo(
    () => ({ favorites, hidden, toggleFavorite, setHidden, error }),
    [favorites, hidden, toggleFavorite, setHidden, error],
  );
}
