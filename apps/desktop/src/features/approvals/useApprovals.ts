import { useCallback, useState } from "react";
import {
  sampleApprovalHistory,
  samplePendingApprovals,
  type ApprovalDecision,
  type ApprovalRecord,
  type ApprovalRequest,
} from "./sample";

export type DecideApproval = (request: ApprovalRequest, decision: ApprovalDecision, rememberForSession: boolean) => void;

export type ApprovalsState = {
  pending: readonly ApprovalRequest[];
  history: readonly ApprovalRecord[];
  decide: DecideApproval;
};

/** Local approval queue: deciding moves a request from pending to history. Nothing reaches the broker yet. */
export function useApprovals(): ApprovalsState {
  const [pending, setPending] = useState<readonly ApprovalRequest[]>(samplePendingApprovals);
  const [history, setHistory] = useState<readonly ApprovalRecord[]>(sampleApprovalHistory);

  const decide = useCallback<DecideApproval>((request, decision, rememberForSession) => {
    setPending((current) => current.filter((item) => item.id !== request.id));
    setHistory((current) => [{ ...request, decision, rememberForSession, decided: "Just now" }, ...current]);
  }, []);

  return { pending, history, decide };
}
