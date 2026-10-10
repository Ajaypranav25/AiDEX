import { EmptyState, List, ListRow, SectionHeader } from "@aidex/ui/components";
import { BotIcon, ChatIcon, ChevronRightIcon, HistoryIcon, PinIcon } from "@aidex/ui/icons";
import { Badge, Button, IconBadge, Stack } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { usePreferences } from "../../app/providers/preferences";
import type { SampleSession } from "../../lib/sample/sessions";
import type { SampleWorkspace } from "../../lib/sample/workspaces";

type Props = { workspace: SampleWorkspace; sessions: readonly SampleSession[] };

/** Chats and agent runs that worked in this workspace. */
export function SessionsPanel({ workspace, sessions }: Props) {
  const { navigate } = useNavigation();
  const { setWorkspaceId } = usePreferences();

  const startChat = () => {
    setWorkspaceId(workspace.id);
    navigate({ page: "chat" });
  };

  return (
    <section aria-labelledby="workspace-sessions-title">
      <Stack gap={3}>
        <SectionHeader
          title={<span id="workspace-sessions-title">Sessions in {workspace.name}</span>}
          meta={`${sessions.length} ${sessions.length === 1 ? "session" : "sessions"}`}
          actions={
            <Button
              variant="ghost"
              size="md"
              trailing={<ChevronRightIcon />}
              aria-label="Open all session history"
              onClick={() => navigate({ page: "history" })}
            >
              All history
            </Button>
          }
        />
        {sessions.length > 0 ? (
          <List metaWidth="140px">
            {sessions.map((session) => (
              <ListRow
                key={session.id}
                media={<IconBadge size="md" icon={session.kind === "agent" ? <BotIcon /> : <ChatIcon />} />}
                title={
                  <>
                    {session.title}{" "}
                    {session.pinned ? (
                      <Badge tone="neutral" icon={<PinIcon />}>
                        Pinned
                      </Badge>
                    ) : null}
                  </>
                }
                subtitle={session.model}
                meta={[session.kind === "agent" ? "Agent run" : "Chat", session.time]}
                onSelect={() => navigate({ page: session.kind, id: session.id })}
              />
            ))}
          </List>
        ) : (
          <EmptyState
            icon={<HistoryIcon size={28} />}
            title="No sessions yet"
            description={`Chats and agent runs that use ${workspace.name} will appear here.`}
            actions={
              <Button variant="outline" size="md" icon={<ChatIcon />} onClick={startChat}>
                Start a chat
              </Button>
            }
          />
        )}
      </Stack>
    </section>
  );
}
