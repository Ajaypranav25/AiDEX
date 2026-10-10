// Sample conversations and agent runs shared by Home, Chat, History, and workspace pages.
export type SampleSession = {
  id: string;
  title: string;
  kind: "chat" | "agent";
  workspaceId: string;
  /** Human-readable time, e.g. "Today, 10:24 AM". */
  time: string;
  model: string;
  pinned?: boolean;
};

export const sampleSessions: readonly SampleSession[] = [
  { id: "s-design-system", title: "Design system discussion", kind: "chat", workspaceId: "ai-dex", time: "Today, 11:02 AM", model: "Gemma 4", pinned: true },
  { id: "s-auth-flow", title: "Fix authentication flow", kind: "agent", workspaceId: "finapp", time: "Today, 9:12 AM", model: "Qwen2.5 Coder 7B", pinned: true },
  { id: "s-api-review", title: "Review API design", kind: "chat", workspaceId: "ai-dex", time: "Today, 8:41 AM", model: "Gemma 4" },
  { id: "s-q3-roadmap", title: "Q3 roadmap planning", kind: "chat", workspaceId: "ai-dex", time: "Yesterday, 5:20 PM", model: "Provider model" },
  { id: "s-lattice-arch", title: "Lattice architecture", kind: "chat", workspaceId: "lattice", time: "Yesterday, 2:05 PM", model: "Gemma 4" },
  { id: "s-sandbox", title: "Improve agent sandbox", kind: "agent", workspaceId: "ai-dex", time: "Yesterday, 1:10 PM", model: "Qwen2.5 Coder 7B" },
  { id: "s-bug-analysis", title: "Bug analysis and fix", kind: "agent", workspaceId: "z0d1ak", time: "Tuesday, 4:48 PM", model: "Qwen2.5 Coder 7B" },
  { id: "s-model-eval", title: "Model evaluation notes", kind: "chat", workspaceId: "tony", time: "Tuesday, 3:31 PM", model: "Llama 3.2 3B" },
  { id: "s-church-street", title: "Church Street timing plan", kind: "chat", workspaceId: "olympus", time: "Monday, 10:15 AM", model: "Gemma 4" },
  { id: "s-email-summaries", title: "Schedule email summaries", kind: "agent", workspaceId: "oddwriter", time: "Monday, 9:00 AM", model: "Gemma 4" },
];

export const sessionById = (id: string | undefined): SampleSession =>
  sampleSessions.find((session) => session.id === id) ?? sampleSessions[0];
