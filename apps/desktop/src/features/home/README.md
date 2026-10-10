# Home feature

Owns the desktop landing screen and its view-specific content. Cross-feature navigation and shared shell remain in `src/app` and `src/layouts`.

`SystemOverview` shows live AiDEX Core status (state, uptime, pid, IPC version, with Start when stopped), drive health (volume, schema, free space, attachment, migration), and a host summary (OS, CPUs, strongest sandbox level). While AiDEX Core runs, `WorkspaceList` shows the drive's workspace registry (`workspace.list`, most recently opened first, selected and unavailable roots marked) and `LiveEvents` (`../system`) shows the last few events and then follows `events.subscribe`. The sidebar's pinned workspaces and recents are still labelled sample data, because the workspace screens they open are samples.