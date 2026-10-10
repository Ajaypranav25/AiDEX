# Desktop features

Feature modules are reserved for UI behavior backed by real AiDEX Core APIs. No backend calls or mock runtime are present in this shell. Implement each feature with typed API contracts once its Core endpoint is defined.

- `chat`: conversation composition and streamed responses.
- `agent`: agent controls and activity presentation.
- `workspace`: project and workspace selection.
- `terminal`: mediated terminal session UI.
- `editor`: workspace file editing.
- `diffs`: change review.
- `tasks`: durable task tracking.
- `approvals`: human review of sensitive actions.
- `checkpoints`: durable execution recovery controls.
- `model-manager`: model discovery and selection.
- `remote-sessions`: paired mobile session controls.
