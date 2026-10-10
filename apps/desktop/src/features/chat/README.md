# Chat feature

Chat mode (route `chat`, `id` = session id), composed only from `@aidex/ui`.

- `ChatPage.tsx`: conversation header (session title, workspace in context, "Continue in Agent mode" and "More options"), the thread, and the docked composer. A prompt handed over from Home (`pendingPrompt`) opens a new chat and is cleared on arrival. Sent messages stay in local state and get an honest preview reply; no model is called.
- `ChatThread.tsx`: sample history for the session plus local messages, rendered with `ChatMessage`, `InlineCode`, and `CodeBlock`.
- `ChatComposer.tsx`: `Composer` with removable attachment chips, the Chat/Agent switch (Agent carries the draft over via `pendingPrompt`), the model picker bound to `usePreferences().modelId`, and the preview note.
- `ChatSidebar.tsx`: New chat, Pinned and Recent sessions, and Workspaces (selecting one changes the chat's context workspace).
- `sample.ts`: sample exchanges and attachable files until AiDEX Core streams real sessions.
