# Security feature

The Security page (`route.page === "security"`) shows the policy, vault, sandbox, and approval state that governs what agents can do. Tabs come from `pageTabs.security` and switch with `replace({ ...route, tab })`.

| File | Shows |
| --- | --- |
| `SecurityPage.tsx` | Header, underline tabs, and the tab panels. Owns the approval queue (`useApprovals`) and isolation level so the dashboard and tab count stay in step. |
| `SecuritySidebar.tsx` | One item per tab (active by route tab; Approvals shows the sample pending count) and recent alerts that open the related tab. |
| `OverviewPanel.tsx` | Security dashboard: posture tiles, active permissions with local Revoke, recent audit log, and the broker explainer. |
| `VaultPanel.tsx`, `AddSecretForm.tsx` | Vault status, masked secrets table, add-secret form, and connected accounts. |
| `SandboxPanel.tsx`, `PolicyRules.tsx` | Isolation level, per-platform backends, and filesystem and network rules. |
| `tabs.ts`, `useHostPlatform.ts`, `sample.ts` | Tab labels and icons, host platform detection, and sample data. |

The Approvals tab renders `../approvals/ApprovalsPanel`.

## Honesty rules

- Everything is sample data until AiDEX Core exposes the policy engine, vault, sandbox, and audit log (`crates/policy`, `crates/vault`, `crates/sandbox`, `crates/broker`). Actions that need Core show a "Preview: …" toast; local-only state (revoking a permission, choosing an isolation level, approval decisions) is not persisted.
- Secrets are never shown in full. The add-secret form validates, clears the value from state, and never stores, logs, or sends it.
- Copy follows `docs/spec.md` and `docs/security.md`: sensitive actions pass through the execution broker, the vault design is Argon2id plus XChaCha20-Poly1305 records, agents get secret handles rather than values, and isolation fails closed when a host can't enforce the chosen level.
