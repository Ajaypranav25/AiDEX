# AiDEX architecture

## Product boundary

The desktop UI, installer, command-line surface, and future mobile controller are clients of AiDEX Core. Durable portable data belongs on the drive; host-specific process and hardware state is discovered and recreated. Agents express typed intents and sensitive actions pass through the execution broker and policy/approval boundary. See the product specification for the complete target architecture and threat model.

## Repository layers

```text
apps/desktop, apps/installer
           │
           ├── shared TypeScript packages
           └── versioned protocol contracts
                         │
                         ▼
                 local AiDEX Core API
                         │
       ┌─────────────────┼────────────────┐
       ▼                 ▼                ▼
 sessions/events    host/runtime     agent harness
       │                                  │
       ▼                                  ▼
    storage                       typed tools / broker
                                          │
                              policy / approval / sandbox
```

The apps are independent Tauri v2 projects. Their Rust Tauri shells own desktop integration only; domain services belong in the Rust workspace, not duplicated inside each UI shell. `protocol/` is the intended source of frontend/backend contracts. Until generation and compatibility checks exist, no handwritten duplicate contract should be treated as canonical.

## Rust workspace boundaries

The first implementation layer is intentionally small: shared core types, portable-drive paths, host capability discovery, events, durable storage/session foundations, and the `aidexd` process boundary. Depend on lower-level shared contracts rather than creating cycles. Future domains listed in the specification—harness, inference, providers/router, context/memory/indexing, tools and adapters, MCP/skills/plugins, broker/policy/sandbox, vault/identity/approvals/remote, cleanup/updater—must be introduced when there is a real API and consumer, not as empty service crates.

## Portable-drive data is not repository source

`portable/` describes an installable drive distribution. It is distinct from this source repository and must never contain user secrets, model weights, or generated binaries in source control. Per-drive state is runtime data (including vault, identity, sessions, and memory); bundled public assets are versioned or verified through manifests. The installer must not claim that a drive is ready until provisioning, integrity checks, and recovery semantics exist.

## Security invariants

- Agent output and project content are untrusted input, not policy.
- No agent gains unrestricted shell or filesystem access by default.
- Secret access is mediated and scoped; raw secrets must not enter model context or logs.
- Remote control uses authenticated, replay-resistant, end-to-end protected messages; a relay is transport only.
- AiDEX can report and clean its own managed residue, but cannot promise removal of host-OS telemetry.
- A new host uses conservative behavior until trust and capabilities are established.

These are design constraints, not claims that the corresponding runtime enforcement is already implemented.
