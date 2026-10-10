import { ActivityLog, InlineCode, KeyValueList, SectionHeader } from "@aidex/ui/components";
import { KeyIcon, PlayIcon, RefreshIcon, XIcon } from "@aidex/ui/icons";
import { Badge, Button, Stack, StatusIndicator, Text } from "@aidex/ui/primitives";
import { useToast } from "../../app/providers/toast";
import type { McpServer, ServerStatus } from "./sample";

type Props = { server: McpServer };

const statusTone: Record<ServerStatus, "success" | "warning" | "neutral"> = {
  Connected: "success",
  "Needs authentication": "warning",
  Stopped: "neutral",
};

export function ServerStatusIndicator({ status }: { status: ServerStatus }) {
  return <StatusIndicator tone={statusTone[status]} label={status} />;
}

const preview = (action: string) => `Preview: ${action} will be available when AiDEX Core is connected.`;

function ServerActions({ server }: Props) {
  const notify = useToast();
  if (server.status === "Stopped") {
    return (
      <Button variant="outline" icon={<PlayIcon />} onClick={() => notify(preview(`starting ${server.name}`))}>
        Start
      </Button>
    );
  }
  return (
    <>
      <Button variant="outline" icon={<RefreshIcon />} onClick={() => notify(preview(`restarting ${server.name}`))}>
        Restart
      </Button>
      <Button variant="ghost" icon={<XIcon />} onClick={() => notify(preview(`disconnecting ${server.name}`))}>
        Disconnect
      </Button>
    </>
  );
}

/** Detail pane for the selected MCP server: connection, tools, authentication, and recent log. */
export function McpServerDetail({ server }: Props) {
  const notify = useToast();
  const headingId = `mcp-detail-${server.id}`;
  const authenticated = !server.auth.required;

  return (
    <section aria-labelledby={headingId}>
      <Stack gap={6}>
        <SectionHeader
          title={<span id={headingId}>{server.name}</span>}
          actions={
            <Stack direction="row" gap={2}>
              <ServerActions server={server} />
            </Stack>
          }
        />
        <KeyValueList
          items={[
            { label: "Status", value: <ServerStatusIndicator status={server.status} /> },
            { label: "Transport", value: server.transport },
            { label: server.transport === "HTTP" ? "URL" : "Command", value: <InlineCode>{server.endpoint}</InlineCode> },
          ]}
        />
        <Stack gap={3}>
          <SectionHeader level={3} title="Exposed tools" meta={authenticated ? `${server.tools.length} tools` : "Not available"} />
          {authenticated ? (
            <Stack direction="row" gap={2} wrap>
              {server.tools.map((tool) => (
                <Badge key={tool} tone="neutral">
                  {tool}
                </Badge>
              ))}
            </Stack>
          ) : (
            <Text tone="muted">This server's tools are listed once you authenticate. Until then, agents can't use them.</Text>
          )}
        </Stack>
        <Stack gap={3}>
          <SectionHeader
            level={3}
            title="Authentication"
            actions={
              authenticated ? null : (
                <Button
                  variant="outline"
                  icon={<KeyIcon />}
                  onClick={() => notify(preview(`authenticating ${server.name} through the vault`))}
                >
                  Authenticate
                </Button>
              )
            }
          />
          <StatusIndicator tone={authenticated ? "success" : "warning"} label={server.auth.status} />
          <Text tone="muted">Secrets live in the encrypted AiDEX vault and are passed to the server only when it starts.</Text>
        </Stack>
        <Stack gap={3}>
          <SectionHeader level={3} title="Recent log" />
          <ActivityLog aria-label={`${server.name} log`} entries={server.log} />
        </Stack>
      </Stack>
    </section>
  );
}
