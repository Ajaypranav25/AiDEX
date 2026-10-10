import { useRef, useState } from "react";
import { DataTable, EmptyState, InlineCode, SectionHeader, type DataTableColumn } from "@aidex/ui/components";
import { ClipboardCheckIcon, HistoryIcon, ShieldCheckIcon } from "@aidex/ui/icons";
import { Button, Stack, StatusIndicator, TabPanel, Tabs } from "@aidex/ui/primitives";
import { useToast } from "../../app/providers/toast";
import { workspaceById } from "../../lib/sample/workspaces";
import { ApprovalCard } from "./ApprovalCard";
import { RiskBadge } from "./RiskBadge";
import type { ApprovalRecord } from "./sample";
import type { ApprovalsState, DecideApproval } from "./useApprovals";

type View = "pending" | "history";

const TABS_ID = "approvals-view";

const historyColumns: readonly DataTableColumn<ApprovalRecord>[] = [
  { key: "summary", header: "Request", rowHeader: true },
  { key: "command", header: "Command", render: (row) => <InlineCode>{row.command}</InlineCode> },
  { key: "agent", header: "Agent", render: (row) => `${row.agent} · ${workspaceById(row.workspaceId).name}` },
  { key: "risk", header: "Risk", render: (row) => <RiskBadge risk={row.risk} /> },
  {
    key: "decision",
    header: "Decision",
    render: (row) => (
      <StatusIndicator
        tone={row.decision === "approved" ? "success" : "warning"}
        label={`${row.decision === "approved" ? "Approved" : "Denied"}${row.rememberForSession ? " for session" : ""}`}
      />
    ),
  },
  { key: "decided", header: "Decided" },
];

/** Approvals Center: pending broker requests as decision cards, and a local history of decisions. */
export function ApprovalsPanel({ pending, history, decide }: ApprovalsState) {
  const [view, setView] = useState<View>("pending");
  const notify = useToast();
  // The decided card unmounts, so focus moves to the next pending card, or to the empty state's action.
  const actionRefs = useRef(new Map<string, HTMLButtonElement>());
  const viewHistoryRef = useRef<HTMLButtonElement>(null);
  const registerAction = (id: string) => (element: HTMLButtonElement | null) => {
    if (element) actionRefs.current.set(id, element);
    else actionRefs.current.delete(id);
  };

  const focusAfterDecision = (decidedId: string) => {
    const index = pending.findIndex((item) => item.id === decidedId);
    const nextId = (pending[index + 1] ?? pending[index - 1])?.id;
    requestAnimationFrame(() => (nextId ? actionRefs.current.get(nextId) : viewHistoryRef.current)?.focus());
  };

  const handleDecide: DecideApproval = (request, decision, rememberForSession) => {
    decide(request, decision, rememberForSession);
    focusAfterDecision(request.id);
    const verb = decision === "approved" ? "Approved" : "Denied";
    const scope = rememberForSession ? " for this session" : "";
    notify(`Preview: ${verb.toLowerCase()} "${request.command}"${scope}. The broker will act on decisions once AiDEX Core is connected.`);
  };

  return (
    <Stack gap={6}>
      <Tabs
        id={TABS_ID}
        label="Approval views"
        variant="pill"
        items={[
          { value: "pending", label: "Pending", icon: <ClipboardCheckIcon />, count: pending.length },
          { value: "history", label: "History", icon: <HistoryIcon />, count: history.length },
        ]}
        value={view}
        onValueChange={setView}
      />
      <TabPanel tabsId={TABS_ID} value={view}>
        {view === "pending" ? (
          <section>
            <Stack gap={4}>
              <SectionHeader title="Waiting for your decision" meta={`${pending.length} pending`} />
              {pending.length === 0 ? (
                <EmptyState
                  icon={<ShieldCheckIcon size={28} />}
                  title="No pending approvals"
                  titleLevel={3}
                  description="When an agent asks the execution broker for a sensitive action, the request waits here for your decision."
                  actions={
                    <Button ref={viewHistoryRef} variant="outline" size="md" onClick={() => setView("history")}>
                      View history
                    </Button>
                  }
                />
              ) : (
                pending.map((request) => (
                  <ApprovalCard key={request.id} request={request} onDecide={handleDecide} firstActionRef={registerAction(request.id)} />
                ))
              )}
            </Stack>
          </section>
        ) : (
          <section>
            <Stack gap={4}>
              <SectionHeader title="Decision history" meta={`${history.length} decisions`} />
              <DataTable
                caption="Approval decisions"
                hideCaption
                columns={historyColumns}
                rows={history}
                rowKey={(row) => row.id}
                emptyText="No decisions yet."
              />
            </Stack>
          </section>
        )}
      </TabPanel>
    </Stack>
  );
}
