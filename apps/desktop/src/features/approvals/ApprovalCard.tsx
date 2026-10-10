import { useState, type Ref } from "react";
import { MediaItem, Terminal } from "@aidex/ui/components";
import { CheckIcon, TerminalIcon, XIcon } from "@aidex/ui/icons";
import { Button, Card, Checkbox, IconBadge, Stack, Text } from "@aidex/ui/primitives";
import { workspaceById } from "../../lib/sample/workspaces";
import { RiskBadge } from "./RiskBadge";
import type { ApprovalRequest } from "./sample";
import type { DecideApproval } from "./useApprovals";

type Props = {
  request: ApprovalRequest;
  onDecide: DecideApproval;
  /** Attached to the first decision button so the panel can move focus here when another card is decided. */
  firstActionRef?: Ref<HTMLButtonElement>;
};

/** One human-readable approval request: what the agent wants to run, where, its effect, and the decision controls. */
export function ApprovalCard({ request, onDecide, firstActionRef }: Props) {
  const [rememberForSession, setRememberForSession] = useState(false);
  const workspace = workspaceById(request.workspaceId);

  return (
    <Card padding="md">
      <Stack gap={5}>
        <MediaItem
          media={<IconBadge icon={<TerminalIcon />} tone={request.risk === "high" ? "accent" : "neutral"} />}
          title={request.summary}
          titleLevel={3}
          description={`${request.agent} · ${workspace.name} workspace · Requested ${request.requested}`}
          trailing={<RiskBadge risk={request.risk} />}
        />
        <Terminal title="Requested command" lines={[{ kind: "command", text: request.command }]} />
        <Text tone="secondary">{request.effect}</Text>
        <Stack direction="row" gap={4} align="center" justify="between" wrap>
          <Checkbox
            label="Remember for this session"
            description="Apply this decision to the same command from this agent until the session ends."
            checked={rememberForSession}
            onChange={(event) => setRememberForSession(event.target.checked)}
          />
          <Stack direction="row" gap={3}>
            <Button
              ref={firstActionRef}
              variant="outline"
              size="md"
              icon={<XIcon />}
              aria-label={`Deny: ${request.summary}`}
              onClick={() => onDecide(request, "denied", rememberForSession)}
            >
              Deny
            </Button>
            <Button
              variant="primary"
              size="md"
              icon={<CheckIcon />}
              aria-label={`Approve: ${request.summary}`}
              onClick={() => onDecide(request, "approved", rememberForSession)}
            >
              Approve
            </Button>
          </Stack>
        </Stack>
      </Stack>
    </Card>
  );
}
