import { useState } from "react";
import { useNavigation } from "../../app/providers/navigation";
import { usePreferences } from "../../app/providers/preferences";
import { sessionById } from "../../lib/sample/sessions";
import { DEFAULT_TASK } from "./sample";

/**
 * Name of the agent task on screen: a prompt handed over on arrival, else the session's title, else the sample task.
 * The page and the sidebar both capture the handed-over prompt when they mount, so they agree even after the page clears it.
 */
export function useTaskName(): string {
  const { route } = useNavigation();
  const { pendingPrompt } = usePreferences();
  const [arrival] = useState(() => ({ prompt: pendingPrompt, routeId: route.id }));

  if (arrival.prompt && arrival.routeId === route.id) return arrival.prompt;
  return route.id ? sessionById(route.id).title : DEFAULT_TASK;
}
