import { useState } from "react";
import { List, ListRow, SectionHeader } from "@aidex/ui/components";
import { ServerIcon } from "@aidex/ui/icons";
import { IconBadge, Split } from "@aidex/ui/primitives";
import { McpServerDetail, ServerStatusIndicator } from "./McpServerDetail";
import { mcpServers } from "./sample";

type Props = {
  /** Server to select on arrival, e.g. from the sidebar; defaults to the first. */
  initialServerId?: string;
};

/** MCP servers tab: server list beside the selected server's details. */
export function McpPanel({ initialServerId }: Props) {
  const [selectedId, setSelectedId] = useState(initialServerId);
  const selected = mcpServers.find((server) => server.id === selectedId) ?? mcpServers[0];
  const connected = mcpServers.filter((server) => server.status === "Connected").length;

  return (
    <Split divided emphasis="end">
      <section aria-labelledby="mcp-servers-heading">
        <SectionHeader title={<span id="mcp-servers-heading">Servers</span>} meta={`${connected} of ${mcpServers.length} connected`} />
        <List>
          {mcpServers.map((server) => (
            <ListRow
              key={server.id}
              media={<IconBadge size="md" icon={<ServerIcon />} />}
              title={server.name}
              subtitle={`${server.transport} · ${server.description}`}
              meta={[<ServerStatusIndicator key="status" status={server.status} />]}
              selected={server.id === selected.id}
              onSelect={() => setSelectedId(server.id)}
            />
          ))}
        </List>
      </section>
      <McpServerDetail server={selected} />
    </Split>
  );
}
