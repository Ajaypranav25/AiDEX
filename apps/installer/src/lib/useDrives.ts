import { useCallback, useEffect, useState } from "react";
import type { VolumeInfo } from "@aidex/client/dto/drive";
import { toAidexError, type AidexError } from "@aidex/client/errors";
import { listDrives } from "@aidex/client/installer";

export type DrivesQuery = {
  volumes: readonly VolumeInfo[];
  error: AidexError | null;
  loading: boolean;
  rescan: () => void;
};

/** Mounted volumes from `list_drives`, loaded on start and on `rescan`. Read only. */
export function useDrives(): DrivesQuery {
  const [volumes, setVolumes] = useState<readonly VolumeInfo[]>([]);
  const [error, setError] = useState<AidexError | null>(null);
  const [loading, setLoading] = useState(true);
  const [tick, setTick] = useState(0);

  useEffect(() => {
    let current = true;
    setLoading(true);
    listDrives().then(
      (found) => {
        if (!current) return;
        setVolumes(found);
        setError(null);
        setLoading(false);
      },
      (caught: unknown) => {
        if (!current) return;
        setError(toAidexError(caught));
        setLoading(false);
      },
    );
    return () => {
      current = false;
    };
  }, [tick]);

  const rescan = useCallback(() => setTick((value) => value + 1), []);
  return { volumes, error, loading, rescan };
}
