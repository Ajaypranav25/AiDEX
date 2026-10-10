import { ActivityLog, EmptyState, Notice } from "@aidex/ui/components";
import { ActivityIcon } from "@aidex/ui/icons";
import type { AidexEvent } from "@aidex/client/dto/daemon";
import { useRpc } from "../../hooks/useRpc";

type Props = {
  /** A real workspace id (the workspace registry, issue #11, supplies these). */
  workspaceId: string;
  /** Newest events shown. */
  limit?: number;
};

const outcomeTone = (event: AidexEvent) =>
  event.kind.outcome === "succeeded" ? "success" : event.kind.outcome === "failed" ? "danger" : "default";

/** What happened, in words, for any event type (the catalog grows with events v2). */
function describe(event: AidexEvent): string {
  const { type, ...fields } = event.kind;
  const detail = Object.entries(fields)
    .map(([key, value]) => `${key.replace(/_/g, " ")}: ${String(value)}`)
    .join(", ");
  const name = type.replace(/_/g, " ");
  return detail ? `${name} (${detail})` : name;
}

/** The latest durable events of one workspace from `events.replay`. */
export function RecentEvents({ workspaceId, limit = 20 }: Props) {
  const replay = useRpc("events.replay", { workspace_id: workspaceId, after: 0 });

  if (replay.error) return <Notice tone="warning">Cannot read events for {workspaceId}: {replay.error.message}</Notice>;
  if (!replay.data) return <Notice>Reading events…</Notice>;
  const events = replay.data.events.slice(-limit).reverse();
  if (events.length === 0) {
    return <EmptyState icon={<ActivityIcon />} title="No events yet" description={`Nothing has been recorded for ${workspaceId}.`} />;
  }
  return (
    <ActivityLog
      entries={events.map((event) => ({
        id: String(event.sequence),
        time: `#${event.sequence}`,
        text: describe(event),
        tone: outcomeTone(event),
      }))}
    />
  );
}
