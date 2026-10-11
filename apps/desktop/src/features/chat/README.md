# Chat feature

Conversations (route `chat`, `id` = session id), composed only from `@aidex/ui`. `ChatPage` and the agent feature's `AgentPage` both render `ConversationPage` with their initial mode.

- `ConversationPage.tsx`: the conversation log, approvals, run errors, and the docked compact `Composer` with mode, model, and project controls. A prompt handed over from Home (`pendingPrompt`) sends itself once an explicit or default model is ready.
- `ConversationModelPicker.tsx`: `ModelPickerTrigger` and `ModelPicker` from `@aidex/ui` over the shared catalog (`useProviderCatalog()`), with favorites, provider status, and links to Providers. `Mod+Shift+M` (`MODEL_PICKER_SHORTCUT` in `lib/shortcuts.ts`) opens and closes it; runtime model lists are fetched when it opens.
- `resolveConversationModel.ts`: pure model resolution. A conversation keeps its own model, shown as Unavailable when it is no longer listed rather than replaced. A new chat uses its draft choice, then the saved default (`preferences.modelId`), then the registry default, then a running local model; it never falls back to the first listed remote model.
- `currentModel.ts`: the picker row and rail entry for the resolved model, including unlisted and hidden ones.
- `sessionConfig.ts`: full `SessionConfig` values (the daemon replaces the whole configuration) and change detection. A conversation keeps its own instructions, fallback, and limits instead of inheriting preferences.
- `ModelStateNotice.tsx`: why the conversation cannot send yet (no model, unavailable model, local model stopped), with the next step.
- `ChatSidebar.tsx`: New chat, Pinned and Recent sessions, and Workspaces.

Choosing a model in an existing conversation calls `agent.configuration.set` with the full configuration and leaves the default for new chats alone. In a new chat the choice is the chat's draft and also becomes `preferences.modelId`. External runtime models switch the conversation to Agent mode with full access and need a project. Choosing a model without native tools while in Agent mode switches to Chat in the same update and says so in a toast. If a conversation was created but its first message failed, changes made since are applied before the retry.
