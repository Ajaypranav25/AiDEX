import { useCallback, useState } from "react";
import { ActivityLog, EmptyState, Notice } from "@aidex/ui/components";
import { ActivityIcon } from "@aidex/ui/icons";
import type { EventEnvelope, EventStreamError } from "@aidex/client/dto/daemon";
import { useRpc } from "../../hooks/useRpc";
import { useSubscription } from "../../hooks/useSubscription";
import { activityEntry } from "./events";

type Props = {
  /** Events kept on screen, newest first. */
  limit?: number;
};

/** Recent history (the last `limit` cursors) followed by live events from `events.subscribe`. */
export function LiveEvents({ limit = 12 }: Props) {
  const latest = useRpc("events.latest");
  const [events, setEvents] = useState<readonly EventEnvelope[]>([]);
  const [streamError, setStreamError] = useState<EventStreamError | null>(null);
  const after = latest.data ? Math.max(0, latest.data.latest_cursor - limit) : null;

  const onData = useCallback(
    (data: unknown, method: string) => {
      if (method === "events.error") {
        setStreamError(data as EventStreamError);
        return;
      }
      if (method !== "events.event") return;
      const envelope = data as EventEnvelope;
      setEvents((current) => [envelope, ...current.filter((existing) => existing.cursor !== envelope.cursor)].slice(0, limit));
    },
    [limit],
  );
  const stream = useSubscription("events.subscribe", { after: after ?? 0 }, onData, { enabled: after !== null });

  const problem = latest.error ?? stream.error;
  if (problem) return <Notice tone="warning">Cannot follow events: {problem.message}</Notice>;
  return (
    <>
      {streamError ? <Notice tone="warning">The event stream stopped: {streamError.message}</Notice> : null}
      {events.length ? (
        <ActivityLog live entries={events.map(activityEntry)} />
      ) : (
        <EmptyState
          icon={<ActivityIcon />}
          title={stream.active ? "Waiting for activity" : "Connecting to the event stream…"}
          description="Sessions, tools, approvals, and drive events appear here as they happen."
        />
      )}
    </>
  );
}
