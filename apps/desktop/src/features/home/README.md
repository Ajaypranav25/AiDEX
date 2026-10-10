# Home feature

Owns the desktop landing screen and its view-specific content. Cross-feature navigation and shared shell remain in `src/app` and `src/layouts`.

`SystemOverview` shows live AiDEX Core status (state, uptime, pid, IPC version, with Start when stopped), drive health (volume, schema, free space, attachment, migration), and a host summary (OS, CPUs, strongest sandbox level). Recent events need a real workspace id from the workspace registry (issue #11); until it lands Home shows none, and Settings Diagnostics reads a workspace's events by id. The sidebar's pinned workspaces and recents are labelled sample data.