// Sample conversation content until AiDEX Core streams real chat sessions.

/** A run of text; `{ code }` renders as inline code. */
export type Segment = string | { code: string };

export type SampleExchange = {
  prompt: string;
  /** Paragraphs before the code sample. */
  intro: readonly (readonly Segment[])[];
  code: { title: string; language: string; code: string };
  /** Paragraph after the code sample. */
  closing: readonly Segment[];
};

const exchanges: Record<string, SampleExchange> = {
  "s-design-system": {
    prompt: "How should the desktop screens share layout without each page writing its own CSS?",
    intro: [
      [
        "Keep every visual decision in ",
        { code: "@aidex/ui" },
        " and let screens only compose components. Pages pick a frame such as ",
        { code: "PageFrame" },
        " or ",
        { code: "DockLayout" },
        ", then arrange content with ",
        { code: "Stack" },
        " and ",
        { code: "Split" },
        ".",
      ],
      ["Tokens stay the single source for color, spacing, and type, so an accent change re-themes every screen at once."],
    ],
    code: {
      title: "features/agent/AgentPage.tsx",
      language: "tsx",
      code: `<DockLayout width="default" dock={<Composer size="compact" … />}>
  <PageFrame>
    <PageHeader title="Implement authentication" />
    <Split divided>{plan}{activity}</Split>
  </PageFrame>
</DockLayout>`,
    },
    closing: ["If a screen needs something new, add it to the package first so the next screen can reuse it."],
  },
  "s-api-review": {
    prompt: "Can you review how the session endpoints handle expired tokens?",
    intro: [
      [
        "The main gap is in ",
        { code: "refreshSession" },
        ": it returns ",
        { code: "null" },
        " for an expired session instead of rotating the refresh token, so clients fall back to a full sign-in.",
      ],
      ["Rotate the token when the session is missing or expired, and keep the response shape the same for callers."],
    ],
    code: {
      title: "src/auth/session.ts",
      language: "ts",
      code: `export async function refreshSession(token: string) {
  const session = await store.get(token);
  if (!session || session.expired) {
    return await rotateRefreshToken(token);
  }
  return session;
}`,
    },
    closing: ["Agent mode can apply this change, update the tests, and show the diff for review."],
  },
};

/** Builds a generic exchange for sessions without hand-written content. */
function genericExchange(title: string): SampleExchange {
  return {
    prompt: `Let's pick up "${title}". Where did we leave off?`,
    intro: [
      [
        "This is a sample conversation in the AiDEX interface. Attach files, choose a workspace for context, or switch to Agent mode to continue the work.",
      ],
      ["Workspace notes are read from ", { code: ".aidex/notes.md" }, " when the workspace is in context, for example:"],
    ],
    code: {
      title: ".aidex/notes.md",
      language: "md",
      code: `## ${title}\n- Open questions captured in this chat\n- Next step: draft a plan in Agent mode`,
    },
    closing: ["Replies will come from your selected model once AiDEX Core is connected."],
  };
}

export const exchangeFor = (sessionId: string, title: string): SampleExchange =>
  exchanges[sessionId] ?? genericExchange(title);

/** Files the attach button offers, in order, until all are attached. */
export const attachableFiles = ["src/auth/session.ts", "docs/architecture.md", "package.json"] as const;
