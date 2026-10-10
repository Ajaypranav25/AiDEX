import { List, ListRow, SectionHeader } from "@aidex/ui/components";
import { BotIcon, ChatIcon } from "@aidex/ui/icons";
import { IconBadge, Stack } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import type { SampleSession } from "../../lib/sample/sessions";
import { kindLabel, sessionCountLabel, workspaceNameOf, type DayGroup } from "./historyFilters";

type Props = { group: DayGroup };

/** One day of history (Today, Yesterday, Earlier); each row reopens its chat or agent run. */
export function SessionDayGroup({ group }: Props) {
  const { navigate } = useNavigation();

  const openSession = (session: SampleSession) =>
    navigate({ page: session.kind === "agent" ? "agent" : "chat", id: session.id });

  return (
    <Stack gap={2}>
      <SectionHeader title={group.label} level={3} meta={sessionCountLabel(group.sessions.length)} />
      <List metaWidth="150px">
        {group.sessions.map((session) => (
          <ListRow
            key={session.id}
            media={<IconBadge size="md" icon={session.kind === "agent" ? <BotIcon /> : <ChatIcon />} />}
            title={session.title}
            subtitle={workspaceNameOf(session.workspaceId)}
            meta={[kindLabel[session.kind], session.model, session.time]}
            onSelect={() => openSession(session)}
          />
        ))}
      </List>
    </Stack>
  );
}
