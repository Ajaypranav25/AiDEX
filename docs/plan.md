# Implementation milestones

The repository follows the phases in the product specification. Milestones are capability goals, not promises that directories already implement them.

1. **Foundation** — repository/tooling, drive-relative paths, daemon lifecycle, versioned event/API contracts, desktop and CLI shells.
2. **Portable agent** — workspace access, native agent loop, brokered shell/file tools, durable sessions, local inference.
3. **Security** — vault, broker/policy/approvals, cleanup reporting, host restrictions.
4. **External agents** — CLI adapters and a stable adapter contract.
5. **Remote control** — device identity, pairing, encrypted control protocol, mobile approvals.
6. **Advanced harness** — delegation, task graph, checkpoints, verification, context and memory.
7. **Advanced sandbox and ecosystem** — kernel-backed isolation, network mediation, plugins, skills, MCP, model packs, signed updates.

The first usable vertical slice should establish one end-to-end path with explicit limits: discover a host, open a workspace, run a managed task through the broker, persist state on the drive, and recover it after restart. Security enforcement and portable-state claims must be verified before broadening the slice.
