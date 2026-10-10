import { List, ListRow, SectionHeader } from "@aidex/ui/components";
import { AlertTriangleIcon, CheckCircleIcon, RefreshIcon, XCircleIcon } from "@aidex/ui/icons";
import { Button, Stack, StatusIndicator, type StatusIndicatorProps } from "@aidex/ui/primitives";
import type { ReactNode } from "react";
import { verification, type CheckResult } from "./sample";

const OUTCOME: Record<CheckResult["outcome"], { icon: ReactNode; tone: StatusIndicatorProps["tone"] }> = {
  passed: { icon: <CheckCircleIcon />, tone: "success" },
  warning: { icon: <AlertTriangleIcon />, tone: "warning" },
  failed: { icon: <XCircleIcon />, tone: "neutral" },
};

/** Results of the checks run against the patch; each status is written out, not shown by color alone. */
export function VerificationSection({ onRunTests }: { onRunTests: () => void }) {
  return (
    <section>
      <Stack gap={4}>
        <SectionHeader
          title="Verification"
          meta="Last run 10:31"
          actions={
            <Button variant="ghost" size="md" icon={<RefreshIcon />} onClick={onRunTests}>
              Run again
            </Button>
          }
        />
        <List metaWidth="140px">
          {verification.map(({ id, name, result, detail, outcome }) => (
            <ListRow
              key={id}
              media={OUTCOME[outcome].icon}
              title={name}
              subtitle={detail}
              trailing={<StatusIndicator label={result} tone={OUTCOME[outcome].tone} />}
            />
          ))}
        </List>
      </Stack>
    </section>
  );
}
