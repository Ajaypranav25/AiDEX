# AiDEX architecture

## Product boundary

The CLI is the first active client surface. Its `aidex` Rust binary uses the existing host crate for diagnostics, delegates interactive agent sessions to the pinned Oh My Pi CLI, and runs headless sessions through Oh My Pi's restricted Bun SDK. The desktop and installer Tauri shells remain in place, but the product UI does not own domain behavior.

## Repository layers

```text
crates/aidex-cli (`aidex`)
   ├── aidex-host (host diagnostics)
   └── pinned Oh My Pi runtime (CLI TUI + restricted Bun SDK)

crates/core, drive, events, sessions, storage
   └── Rust foundation; shared local-core API is not implemented yet

apps/desktop, apps/installer
   └── future clients of a versioned local-core API
```

The desktop and installer are independent Tauri v2 projects. Their Rust shells own desktop integration only; domain services belong in the Rust workspace. `crates/aidex-cli` owns the first Rust command surface and treats Oh My Pi as an external agent runtime, not as AiDEX core. `protocol/` is the intended source of frontend/backend contracts. Until generation and compatibility checks exist, no handwritten duplicate contract should be treated as canonical.

## Rust workspace boundaries

The Rust workspace remains the primary implementation focus. Its first layer is intentionally small: shared core types, portable-drive paths, host capability discovery, events, durable storage/session foundations, the `aidexd` process boundary, and the `aidex` CLI. Depend on lower-level shared contracts rather than creating cycles. Future domains listed in the specification—harness, inference, providers/router, context/memory/indexing, tools and adapters, MCP/skills/plugins, broker/policy/sandbox, vault/identity/approvals/remote, cleanup/updater—must be introduced when there is a real API and consumer, not as empty service crates.

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
