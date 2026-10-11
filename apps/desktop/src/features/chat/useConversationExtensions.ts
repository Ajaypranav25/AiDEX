import { useCallback, useEffect, useMemo, useRef } from "react";
import type { PlanStatus } from "@aidex/client/dto/artifacts";
import { NATIVE_TOOL_NAMES, type Message } from "@aidex/client/dto/sessions";
import type { SkillSummary } from "@aidex/client/dto/skills";
import { useRpc } from "../../hooks/useRpc";
import { invocableSkills } from "./composer/skillSearch";
import { webSearchAvailability } from "./composer/useWebSearchToggle";

/**
 * Skills, web search, and plan state a conversation's composer and transcript use: the invocable
 * skills of its project, whether web search is set up, and the current status of proposed plans.
 */
export function useConversationExtensions({
  sessionId,
  workspaceId,
  messages,
}: {
  sessionId: string | undefined;
  /** A registered project; without one only drive and plugin skills are listed. */
  workspaceId: string | undefined;
  messages: readonly Message[] | undefined;
}) {
  const skillList = useRpc("skill.list", workspaceId ? { workspace_id: workspaceId } : undefined);
  const allSkills = skillList.data?.skills;
  const skills = useMemo(() => invocableSkills(allSkills), [allSkills]);
  const skill = useCallback(
    (id: string): SkillSummary | undefined => allSkills?.find((candidate) => candidate.id === id),
    [allSkills],
  );

  const webSettings = useRpc("web.settings.get");
  const web = webSearchAvailability(webSettings.data);

  // Plan cards show whether a newer plan superseded them, so the list follows new tool results.
  const hasPlans = !!messages?.some((message) =>
    message.tool_calls.some((call) => call.name === NATIVE_TOOL_NAMES.planPropose),
  );
  const artifacts = useRpc(
    "artifact.list",
    { session_id: sessionId ?? "", limit: 100 },
    { enabled: !!sessionId && hasPlans },
  );
  const refetchArtifacts = artifacts.refetch;
  const toolResults = messages?.filter((message) => message.role === "tool").length ?? 0;
  const seenResults = useRef(toolResults);
  useEffect(() => {
    if (seenResults.current === toolResults) return;
    seenResults.current = toolResults;
    if (hasPlans) refetchArtifacts();
  }, [toolResults, hasPlans, refetchArtifacts]);
  const artifactList = artifacts.data?.artifacts;
  const planStatus = useCallback(
    (id: string): PlanStatus | undefined =>
      artifactList?.find((artifact) => artifact.id === id)?.plan_status ?? undefined,
    [artifactList],
  );

  return {
    skills,
    skillsLoading: skillList.loading,
    refetchSkills: skillList.refetch,
    skill,
    web,
    refetchWeb: webSettings.refetch,
    planStatus,
  };
}
