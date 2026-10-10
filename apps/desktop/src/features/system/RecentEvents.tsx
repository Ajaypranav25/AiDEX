import { ActivityLog, EmptyState, Notice } from "@aidex/ui/components";
import { ActivityIcon } from "@aidex/ui/icons";
import type { EventFilter } from "@aidex/client/dto/daemon";
import { call } from "@aidex/client/rpc";
import { useAidexStatus } from "../../hooks/useAidexStatus";
import { useBridgeQuery } from "../../hooks/useBridgeQuery";
import { activityEntry } from "./events";

type Props = {
  /** Narrow to one workspace (`wsp_…`); omit for every event. */
  workspaceId?: string;
  /** Newest events shown. */
  limit?: number;
};

/** How far back (in global cursor positions) a filtered read looks for the newest events. */
const LOOKBACK = 500;

/** The newest durable events from `events.latest` plus one `events.replay` page. */
export function RecentEvents({ workspaceId, limit = 20 }: Props) {
  const { connectionKey, daemonRunning } = useAidexStatus();
  const filter: EventFilter = workspaceId ? { workspace_id: workspaceId } : {};
  const recent = useBridgeQuery(
    async () => {
      const latest = await call("events.latest");
      const page = await call("events.replay", { after: Math.max(0, latest.latest_cursor - LOOKBACK), filter });
      return page.events.slice(-limit).reverse();
    },
    `recent|${connectionKey}|${workspaceId ?? "*"}|${limit}`,
    daemonRunning,
  );

  if (recent.error) return <Notice tone="warning">Cannot read events: {recent.error.message}</Notice>;
  if (!recent.data) return <Notice>Reading events…</Notice>;
  if (recent.data.length === 0) {
    return (
      <EmptyState
        icon={<ActivityIcon />}
        title="No events yet"
        description={workspaceId ? `Nothing has been recorded for ${workspaceId}.` : "Nothing has been recorded on this drive yet."}
      />
    );
  }
  return <ActivityLog entries={recent.data.map(activityEntry)} />;
}
