import type { Ref } from "react";
import { ActivityLog, SectionHeader, StepList, type ActivityEntry } from "@aidex/ui/components";
import { ChevronRightIcon, GitCompareIcon, HistoryIcon, LockIcon, PlayIcon, ShieldCheckIcon } from "@aidex/ui/icons";
import { Badge, Button, Stack } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { permissions, stepsAt } from "./sample";

type PlanProps = { completed: number; stepCount: number };

export function PlanSection({ completed, stepCount }: PlanProps) {
  const { navigate } = useNavigation();
  return (
    <section>
      <Stack gap={5} align="start">
        <SectionHeader title="Plan" meta={`${Math.min(completed, stepCount)} / ${stepCount} complete`} />
        <StepList steps={stepsAt(completed)} />
        <Button variant="ghost" size="md" icon={<HistoryIcon />} onClick={() => navigate({ page: "checkpoints" })}>
          Checkpoints
        </Button>
      </Stack>
    </section>
  );
}

type ActivityProps = {
  log: readonly ActivityEntry[];
  canAdvance: boolean;
  onAdvance: () => void;
  /** Focus targets for when the run's controls disable themselves. */
  logRef?: Ref<HTMLOListElement>;
  reviewRef?: Ref<HTMLButtonElement>;
};

export function ActivitySection({ log, canAdvance, onAdvance, logRef, reviewRef }: ActivityProps) {
  const { navigate } = useNavigation();
  return (
    <section>
      <Stack gap={5}>
        <SectionHeader
          title="Activity"
          actions={
            <Button variant="ghost" size="md" trailing={<ChevronRightIcon />} onClick={() => navigate({ page: "execution" })}>
              Full timeline
            </Button>
          }
        />
        <ActivityLog ref={logRef} tabIndex={-1} aria-label="Run activity" entries={log} live />
        <Stack direction="row" gap={3} wrap>
          <Button variant="outline" size="md" icon={<PlayIcon />} disabled={!canAdvance} onClick={onAdvance}>
            Advance demo
          </Button>
          <Button
            ref={reviewRef}
            variant="outline"
            size="md"
            icon={<GitCompareIcon />}
            onClick={() => navigate({ page: "review" })}
          >
            Review diff
          </Button>
        </Stack>
      </Stack>
    </section>
  );
}

export function PermissionsSection() {
  const { navigate } = useNavigation();
  return (
    <section>
      <Stack gap={4}>
        <SectionHeader
          title="Tools and permissions"
          actions={
            <Button variant="ghost" size="md" trailing={<ChevronRightIcon />} onClick={() => navigate({ page: "agents" })}>
              Agent settings
            </Button>
          }
        />
        <Stack direction="row" gap={2} wrap role="list" aria-label="Permissions for this run">
          {permissions.map(({ label, allowed }) => (
            <Badge
              key={label}
              role="listitem"
              tone={allowed ? "accent" : "neutral"}
              icon={allowed ? <ShieldCheckIcon /> : <LockIcon />}
            >
              {label}
            </Badge>
          ))}
        </Stack>
      </Stack>
    </section>
  );
}
