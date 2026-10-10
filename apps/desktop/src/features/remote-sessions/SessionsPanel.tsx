import { useRef } from "react";
import { DataTable, EmptyState, MediaItem, SectionHeader, type DataTableColumn } from "@aidex/ui/components";
import { WifiIcon } from "@aidex/ui/icons";
import { Badge, Button, Card, IconBadge, Stack, StatusIndicator } from "@aidex/ui/primitives";
import { permissionLabels, type RemoteSession } from "./sample";

export type EndAction = "disconnect" | "revoke";

type Props = {
  sessions: readonly RemoteSession[];
  onEnd: (session: RemoteSession, action: EndAction) => void;
};

/** Connection status and the phones currently attached to desktop sessions. */
export function SessionsPanel({ sessions, onEnd }: Props) {
  // An ended row unmounts, so focus moves to a neighbouring row's first action, or to the empty state.
  const actionRefs = useRef(new Map<string, HTMLButtonElement>());
  const emptyRef = useRef<HTMLDivElement>(null);
  const registerAction = (id: string) => (element: HTMLButtonElement | null) => {
    if (element) actionRefs.current.set(id, element);
    else actionRefs.current.delete(id);
  };

  const end = (session: RemoteSession, action: EndAction) => {
    const index = sessions.findIndex((candidate) => candidate.id === session.id);
    const neighbourId = (sessions[index + 1] ?? sessions[index - 1])?.id;
    onEnd(session, action);
    requestAnimationFrame(() => (neighbourId ? actionRefs.current.get(neighbourId) : emptyRef.current)?.focus());
  };

  const columns: readonly DataTableColumn<RemoteSession>[] = [
    { key: "device", header: "Device", rowHeader: true },
    { key: "user", header: "User", width: "64px" },
    { key: "session", header: "Session" },
    { key: "started", header: "Started" },
    {
      key: "status",
      header: "Status",
      render: (row) => <StatusIndicator tone={row.status === "Connected" ? "success" : "neutral"} label={row.status} />,
    },
    { key: "latency", header: "Latency", align: "end" },
    {
      key: "permissions",
      header: "Permissions",
      render: (row) => (
        <Stack direction="row" gap={1} wrap>
          {row.permissions.map((permission) => (
            <Badge key={permission} tone={permission === "can-approve" ? "accent" : "neutral"}>
              {permissionLabels[permission]}
            </Badge>
          ))}
        </Stack>
      ),
    },
    {
      key: "actions",
      header: "Actions",
      render: (row) => (
        <Stack direction="row" gap={1}>
          <Button
            ref={registerAction(row.id)}
            variant="ghost"
            size="md"
            aria-label={`Disconnect ${row.device}`}
            onClick={() => end(row, "disconnect")}
          >
            Disconnect
          </Button>
          <Button variant="ghost" size="md" aria-label={`Revoke ${row.device}`} onClick={() => end(row, "revoke")}>
            Revoke
          </Button>
        </Stack>
      ),
    },
  ];

  return (
    <Stack gap={10}>
      <Card padding="md">
        <Stack gap={4}>
          <MediaItem
            media={<IconBadge icon={<WifiIcon />} tone="accent" />}
            title="Connection status"
            titleLevel={2}
            description="Paired phones reach this desktop directly; nothing leaves your network while the relay is off."
          />
          <Stack direction="row" gap={6} wrap>
            <StatusIndicator tone="success" label="Listening on local network" />
            <StatusIndicator tone="neutral" label="Relay off" />
            <StatusIndicator tone="success" label="Encryption on" />
          </Stack>
        </Stack>
      </Card>
      <section>
        <SectionHeader title="Active remote sessions" meta={`${sessions.length} connected`} />
        {sessions.length === 0 ? (
          <EmptyState
            ref={emptyRef}
            tabIndex={-1}
            icon={<WifiIcon size={32} />}
            title="No remote sessions"
            titleLevel={3}
            description="When a paired phone opens a desktop session, it appears here with its permissions."
          />
        ) : (
          <DataTable
            caption="Active remote sessions"
            hideCaption
            columns={columns}
            rows={sessions}
            rowKey={(row) => row.id}
          />
        )}
      </section>
    </Stack>
  );
}
