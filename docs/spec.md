# AiDEX Technical Specification

> **Project:** AiDEX  
> **Category:** Portable AI development environment / agentic runtime  
> **Status:** Architecture and product specification  
> **Primary form factor:** USB / portable SSD / removable drive  
> **Core idea:** Your complete AI development environment, agent harness, local models, tools, state, security boundary, and remote controls travel with the drive.

---

## 1. Executive Summary

AiDEX is a portable AI developer environment that lives on a removable drive and can be launched on a compatible host computer without requiring the developer to rebuild their AI stack on every machine.

It is not only a launcher for existing tools.

AiDEX is its own full agent application, comparable at the product level to tools such as codex, claude code, antigravity, or an advanced OMP-style harness. It provides its own agent runtime, chat and task interface, tool execution system, subagents, skills, MCP support, memory, model routing, approvals, diffs, terminal sessions, sandboxing, checkpoints, and remote control.

Third-party agents such as codex, claude code, omp, antigravity and other CLI-based agents are treated as pluggable runtimes inside the AiDEX environment.

The drive can also contain local models, including Gemma 4 model packs and GGUF/C++-runtime-compatible models. These models run directly from the removable drive through bundled native inference runtimes. AiDEX dynamically selects CPU, GPU, memory, context size, quantization and offload strategy based on the host machine.

AiDEX also includes a mobile companion that can securely pair with the running drive session. The phone can start tasks, monitor agents, approve sensitive actions, review diffs, pause or terminate jobs, receive notifications, and remotely interact with AiDEX.

The long-term vision is simple:

**Plug in AiDEX. Open it. Your agents, tools, models, skills, MCP servers, configs, encrypted secrets, sessions and development environment are there. Work from anywhere. Unplug and take the environment with you.**

---

# 2. Product Vision

Current AI coding environments are highly machine-dependent.

A serious developer may have:

- codex
- claude code
- omp
- antigravity
- shell configuration
- MCP servers
- authentication
- provider API keys
- plugins
- reusable skills
- coding rules
- system prompts
- custom agents
- project templates
- model runtimes
- local models
- tool permissions
- shell utilities
- language runtimes
- package managers
- compilers
- session histories
- personal workflows
- memory
- repositories
- remote access tools

Moving to another laptop, lab computer, workstation, or temporary environment often means reconstructing a large portion of that setup.

AiDEX makes the *developer's AI environment itself portable*.

The removable drive becomes a self-contained agent workspace that can discover the host, adapt itself to available capabilities, launch the required runtime, and retain its important state on the drive rather than scattering that state across the host.

AiDEX should feel less like "running software from a pendrive" and more like inserting a personal AI development system into a computer.

---

# 3. Non-Goals

AiDEX is **not**:

- a bootable operating system
- a replacement for Windows, Linux, or macOS
- a BIOS-level environment
- a virtual machine image that requires rebooting into it
- only a wrapper around codex or claude code
- only a local LLM frontend
- only a portable terminal
- only an encrypted USB vault
- dependent on a permanent AiDEX installation on the host
- expected to guarantee that a hostile or administrator-controlled host leaves absolutely zero forensic traces

The normal experience is:

1. plug in the drive
2. launch AiDEX
3. AiDEX discovers the host
4. unlock the encrypted workspace
5. choose or resume a project
6. start one or more agents
7. optionally control them remotely from the mobile app
8. end the session
9. cleanup controllable temporary host state
10. unplug the drive

Automatic launching immediately after USB insertion may be supported where the operating system allows it or where an optional AiDEX companion is installed, but AiDEX must not depend on autorun behavior.

---

# 4. Core Design Principles

## 4.1 Portable First

All essential user-owned state must be capable of living on the AiDEX drive:

- agent configurations
- skills
- MCP definitions
- user rules
- model packs
- provider configuration
- encrypted secrets
- session history
- task checkpoints
- prompt libraries
- plugin manifests
- tool policies
- portable binaries
- local memory
- project metadata
- runtime metadata

The host computer is compute, I/O, hardware and workspace access.

The AiDEX drive is identity, environment, runtime and durable state.

## 4.2 Local First, Cloud Optional

AiDEX must remain useful without external model providers.

The default local stack can contain:

- Gemma 4
- GGUF coding models
- embedding models
- rerankers
- lightweight utility models
- optional speech models
- additional llama.cpp-compatible models

Cloud models may be used when available, but local execution is a first-class runtime rather than a degraded afterthought.

## 4.3 Agent Native

AiDEX is built around autonomous tasks rather than only chat.

A session can contain:

- primary agents
- worker agents
- research agents
- coding agents
- verification agents
- test agents
- shell agents
- planning agents
- local-only agents
- cloud-backed agents

Agents can create subagents, delegate tasks, run tools, checkpoint work, request approvals and exchange artifacts.

## 4.4 Security by Mediation

Agents never receive unrestricted execution merely because they are AI agents.

Sensitive operations flow through the AiDEX execution broker.

Examples:

- shell command
- process creation
- file write
- host filesystem mount
- network request
- package installation
- secret access
- git push
- credential use
- destructive command
- remote action

Each can be allowed, denied, sandboxed, logged, scoped, time-limited or require human approval.

## 4.5 Host Adaptive

AiDEX must adapt to the computer it is plugged into.

The same drive may encounter:

- a low-power laptop
- a university lab PC
- a gaming laptop
- a workstation
- a Linux server
- a machine with no discrete GPU
- a machine with a large NVIDIA GPU
- a machine with only integrated graphics
- a machine with missing compilers
- a restricted corporate system

AiDEX probes capabilities before composing the runtime.

## 4.6 Durable Execution

Long agent tasks should survive:

- app restarts
- model changes
- network loss
- provider failure
- host changes
- controlled interruption
- remote disconnection

AiDEX stores durable checkpoints on the drive and should resume from the last valid state wherever possible.

---

# 5. High-Level Architecture

```text
                           +----------------------+
                           |   AiDEX Mobile App   |
                           | approvals / control  |
                           +----------+-----------+
                                      |
                            E2E Encrypted Session
                                      |
                                      v
+---------------------------------------------------------------------+
|                         HOST COMPUTER                               |
|                                                                     |
|  +---------------------------------------------------------------+  |
|  |                     AiDEX Desktop App                          |  |
|  | projects | chat | agents | diffs | terminal | tools | models |  |
|  +-----------------------------+---------------------------------+  |
|                                |                                    |
|                                v                                    |
|  +---------------------------------------------------------------+  |
|  |                      AiDEX Core Daemon                         |  |
|  | session mgr | event bus | policy | models | agents | remote   |  |
|  +-------------+------------------+-------------------+----------+  |
|                |                  |                   |             |
|                v                  v                   v             |
|      +----------------+  +----------------+  +------------------+  |
|      | Agent Harness  |  | Model Runtime  |  | Execution Broker |  |
|      | tools/subagents|  | local + remote |  | sandbox / policy |  |
|      +-------+--------+  +-------+--------+  +---------+--------+  |
|              |                   |                     |           |
|      +-------v-----------------------------------------v--------+  |
|      |                AiDEX Runtime Sandbox                    |  |
|      | process | filesystem | network | secrets | resource QoS |  |
|      +---------------------------+------------------------------+  |
|                                  |                                 |
|              host workspace / compilers / shell / hardware         |
+----------------------------------+----------------------------------+
                                   |
                                   v
+---------------------------------------------------------------------+
|                          AiDEX DRIVE                                |
|                                                                     |
| runtime | models | agents | skills | MCP | vault | sessions | cache |
| checkpoints | binaries | manifests | memory | logs | project state |
+---------------------------------------------------------------------+
```

---

# 6. AiDEX Product Surfaces

AiDEX is a product family rather than one executable.

## 6.1 AiDEX Desktop

The primary graphical application.

Recommended implementation:

- Tauri
- Rust backend
- React frontend
- native terminal integration
- system tray support
- deep integration with AiDEX Core

The desktop application should combine the strengths of:

- coding agent interfaces
- terminal-centric workflows
- IDE agent panels
- task managers
- diff review tools
- multi-agent orchestration dashboards

### Main Desktop Areas

#### Home

Shows:

- recent workspaces
- active sessions
- running agents
- local model status
- remote connectivity
- drive health
- current security mode

#### Workspace

Contains:

- repository tree
- task/chat panel
- live terminal
- tool timeline
- code diff view
- file inspector
- task plan
- subagent graph
- context view
- checkpoints
- approvals

#### Agent Center

Shows all available harnesses and agents:

- AiDEX Native Agent
- codex
- claude code
- omp
- antigravity
- custom CLI agents
- custom MCP-backed agents
- local-only agents

Each adapter exposes capabilities through a standard interface.

#### Model Center

Manages:

- local model packs
- model downloads/imports
- model integrity
- quantization variants
- context limits
- runtime compatibility
- GPU offload
- benchmark results
- cloud providers
- fallback chains

#### Skills Center

Supports:

- install
- enable
- disable
- version
- scope
- permission inspection
- update
- export

#### MCP Center

Supports:

- local stdio MCP servers
- HTTP MCP servers
- remote MCP servers
- authentication
- permissions
- health checks
- per-project enablement

#### Security Center

Shows:

- vault state
- paired devices
- active remote sessions
- granted host mounts
- network rules
- secret grants
- pending approvals
- sandbox level
- audit trail
- residue cleanup report

---

## 6.2 AiDEX CLI

`aidex` is a first-class CLI, not just a launcher.

Example:

```bash
aidex
aidex chat
aidex run "fix the failing tests"
aidex agent list
aidex agent use codex
aidex agent use claude-code
aidex agent use omp
aidex model list
aidex model use gemma4
aidex model benchmark
aidex skills list
aidex mcp list
aidex sandbox shell
aidex session list
aidex session resume <id>
aidex remote pair
aidex vault unlock
aidex doctor
aidex cleanup
```

The CLI should expose nearly all desktop capabilities.

A TUI mode may provide:

- live tool calls
- subagent tree
- token/model stats
- approvals
- diff review
- logs
- shell panes

---

## 6.3 AiDEX Mobile

The mobile companion is a secure remote control surface for active AiDEX sessions.

It is not required for local operation.

### Mobile Capabilities

- see active machines
- see active drives
- see agents
- see current tasks
- send a new prompt
- start a task
- pause agent
- resume agent
- cancel task
- terminate process
- approve or reject shell commands
- approve file writes
- approve git operations
- review diffs
- review plans
- view terminal output
- view logs
- receive completion notifications
- switch models
- change agent
- grant temporary secret use
- revoke a session
- lock the drive
- trigger cleanup
- emergency stop

### Remote Session Modes

1. **LAN Direct**
2. **Peer-to-peer Internet**
3. **Relay-assisted E2E encrypted**
4. **Local QR pairing only**
5. **Optional self-hosted relay**

Remote connectivity should not require the relay to access plaintext task data.

---

# 7. AiDEX Native Agent

AiDEX includes its own primary coding and general development agent.

This means the system remains useful even when codex, claude code, omp or other third-party tools are unavailable.

The native agent must support:

- repository understanding
- planning
- code generation
- code editing
- shell execution
- build/test loops
- git operations
- file search
- semantic search
- documentation retrieval
- browser/tool adapters where available
- MCP
- skills
- subagents
- verification
- checkpoints
- model switching
- context compaction
- remote approvals
- durable execution

The AiDEX native agent can run against:

- local Gemma 4
- another local GGUF model
- OpenAI-compatible endpoint
- Anthropic-compatible provider adapter
- custom provider adapters
- self-hosted models

---

# 8. AiDEX Harness

The AiDEX Harness is the core orchestration layer and should be comparable in power to an advanced OMP-style environment.

## 8.1 Responsibilities

The harness manages:

- model loop
- system prompt assembly
- agent identity
- tool registry
- tool invocation
- MCP routing
- skill loading
- subagent creation
- context management
- memory retrieval
- artifact passing
- approval requests
- retry policies
- budget policies
- checkpoints
- agent lifecycle
- task lifecycle
- error recovery
- verification
- remote event streaming
- telemetry stored locally
- model fallback

## 8.2 Agent Graph

A task is represented as a graph.

```text
User Task
   |
   v
Planner
   |
   +----> Repository Analyst
   |
   +----> Implementation Agent
   |
   +----> Test Agent
   |
   +----> Security Reviewer
   |
   v
Verifier
   |
   v
Final Result
```

The graph can be dynamic.

Agents may create child tasks based on discovered work.

## 8.3 Agent Runtime Interface

Every agent backend should implement a common contract conceptually similar to:

```rust
trait AgentRuntime {
    fn capabilities(&self) -> AgentCapabilities;
    fn start(&self, request: AgentRequest) -> SessionId;
    fn send(&self, session: SessionId, input: AgentInput);
    fn pause(&self, session: SessionId);
    fn resume(&self, session: SessionId);
    fn cancel(&self, session: SessionId);
    fn stream_events(&self, session: SessionId) -> EventStream;
}
```

This abstraction allows AiDEX to host:

- native agents
- command-line agents
- API agents
- local model agents
- remote agents

---

# 9. Third-Party Agent Adapters

Third-party tools are treated as runtimes.

Initial adapters:

- codex
- claude code
- omp
- antigravity
- generic command-line agent
- generic OpenAI-compatible agent endpoint

Each adapter declares:

```yaml
id: codex
kind: cli-agent
binary: runtime/agents/codex/codex
supports:
  interactive: true
  resume: true
  tools: native
  project_context: true
  remote_stream: true
auth:
  mode: delegated
sandbox:
  default: brokered
```

Adapters normalize:

- input
- output
- tool events
- process state
- approvals
- session history
- exit status
- errors
- capabilities

AiDEX should not assume every third-party agent exposes identical features.

---

# 10. Local Model System

One of AiDEX's major differentiators is that local model capability physically travels with the drive.

## 10.1 Default Model Family

The target default local model family is **Gemma 4**.

The exact model size and quantization may differ by drive capacity and host hardware.

Example model packs:

```text
gemma4-code-small-q4
gemma4-general-q4
gemma4-general-q6
gemma4-large-q4
```

The model layer must remain model-agnostic so future Gemma versions and other open-weight models can be added without changing the rest of AiDEX.

## 10.2 C++ Local Runtime

AiDEX bundles native C/C++ model runtimes on the drive.

Primary compatibility target:

- GGUF
- llama.cpp-family runtimes

Optional runtimes may support:

- speech-to-text
- embeddings
- reranking
- vision
- image generation

All binaries are stored per operating system and architecture.

Example:

```text
runtime/
  windows-x64/
    llama/
    embeddings/
  linux-x64/
    llama/
    embeddings/
  linux-arm64/
  macos-arm64/
```

## 10.3 Model Registry

```yaml
id: gemma4-code-q4
family: gemma
version: "4"
format: gguf
file: models/gemma4/code-q4/model.gguf
sha256: ...
capabilities:
  chat: true
  code: true
  tools: true
min_ram_gb: 8
recommended_ram_gb: 16
gpu_offload: auto
context:
  default: 32768
```

## 10.4 Hardware Adaptive Loading

Before model startup, AiDEX measures:

- OS
- CPU architecture
- physical cores
- logical cores
- RAM
- available RAM
- GPU vendor
- GPU model
- VRAM
- supported compute backend
- drive throughput
- thermal/power mode

The model composer selects:

- model
- quantization
- context size
- batch size
- thread count
- memory mapping mode
- GPU layers
- CPU/GPU split
- cache policy

## 10.5 Local Model Roles

Different models can have different roles.

Example:

```text
Gemma 4 coding model -> implementation
small utility model   -> classification / routing
embedding model       -> retrieval
reranker              -> context ranking
Gemma 4 general       -> planning / summarization
```

This avoids loading the largest model for every internal operation.

## 10.6 Model Hot Switching

A running session can switch from:

```text
local Gemma 4
       |
       v
cloud model
       |
       v
local Gemma 4
```

without destroying the task graph.

The harness preserves:

- compressed task context
- tool state
- artifacts
- checkpoints
- open approvals
- task graph

---

# 11. Portable Runtime Composer

When AiDEX starts, it does not blindly launch the same environment everywhere.

It performs host discovery.

## 11.1 Host Probe

AiDEX detects:

- platform
- architecture
- filesystem capabilities
- shell
- installed compilers
- git
- node
- bun
- python
- rust
- docker
- podman
- WSL
- GPU runtimes
- available ports
- sandbox capabilities
- virtualization support
- network state
- writable temp directories
- security restrictions

## 11.2 Capability Manifest

The probe emits:

```yaml
host:
  os: windows
  arch: x86_64
  cpu:
    cores: 16
  ram_gb: 32
  gpu:
    vendor: nvidia
    vram_gb: 8
  tools:
    git: true
    bun: true
    rust: false
  sandbox:
    restricted_token: true
    appcontainer: true
    vm: false
  local_model:
    recommended: gemma4-code-q4
```

## 11.3 Runtime Composition

The runtime composer chooses:

- native executable variants
- sandbox backend
- local model
- toolchain paths
- local cache strategy
- network mode
- available agent adapters
- host integration level

---

# 12. Kernel-Backed Runtime Sandbox

AiDEX should provide a strong sandbox without becoming an operating system.

The sandbox uses operating-system kernel primitives where available.

## 12.1 Security Levels

### Level 0: Direct

Agent commands run directly as the current user.

Used only when explicitly chosen.

### Level 1: Brokered

Every sensitive action passes through the AiDEX Execution Broker.

The agent itself has reduced direct access.

### Level 2: Restricted

Uses native OS restrictions.

Examples conceptually include:

- restricted process token
- job/process limits
- filesystem allowlists
- namespace isolation
- seccomp-style syscall policies
- resource limits
- network controls

### Level 3: Ephemeral Isolation

Where the host supports it, AiDEX may launch a stronger isolated execution environment such as:

- container
- namespace sandbox
- lightweight VM
- OS sandbox facility

AiDEX still remains a normal application. It does not require booting a different OS.

## 12.2 Filesystem Virtualization

An agent sees explicit mounts.

Example:

```text
/workspace  -> C:\Users\User\Projects\foo
/aidex      -> encrypted/session-visible drive state
/tmp        -> ephemeral scratch layer
/tools      -> read-only AiDEX tool bundle
```

Sensitive host directories are not automatically exposed.

## 12.3 Copy-on-Write Mode

For high-risk tasks, AiDEX can place modifications in an overlay.

The user can:

- inspect changes
- accept changes
- reject changes
- partially apply changes
- rollback

This provides a transaction-like workflow for autonomous coding.

## 12.4 Process Broker

All child processes receive:

- session ID
- policy ID
- resource limits
- environment allowlist
- secret handles
- network policy
- output capture

## 12.5 Network Broker

Per-session policies may be:

```yaml
network:
  mode: allowlist
  domains:
    - github.com
    - api.openai.com
  local_network: deny
  arbitrary_ip: deny
```

Modes:

- offline
- allowlist
- normal
- unrestricted

## 12.6 Resource Control

AiDEX can limit:

- CPU
- RAM
- process count
- disk writes
- execution duration
- network bytes
- local model VRAM usage

---

# 13. Execution Broker

The Execution Broker is the security boundary between reasoning and action.

Agents issue structured intents.

Example:

```json
{
  "action": "shell.exec",
  "command": "git push origin feature-x",
  "workspace": "repo-51",
  "risk": "remote-write"
}
```

The broker decides:

```text
ALLOW
DENY
SANDBOX
REQUIRE_APPROVAL
REWRITE
RATE_LIMIT
```

Policies can exist at:

- global level
- machine level
- workspace level
- agent level
- skill level
- tool level
- session level

---

# 14. Approval Engine

Sensitive operations should have human-readable approval cards.

Example:

```text
claude-code wants to execute:

git push origin main

Repository:
Astraive/project

Effect:
Uploads 3 local commits to the remote main branch.

[Approve Once] [Approve For Session] [Deny]
```

Approvals can appear on:

- desktop
- CLI
- mobile

Approval rules can include:

```yaml
approvals:
  shell:
    destructive: always
  git:
    push: always
    commit: auto
  filesystem:
    outside_workspace: always
  secrets:
    use: always
```

---

# 15. Encrypted Vault

AiDEX needs its own portable secret system.

## 15.1 Vault Stores

- API keys
- provider tokens
- MCP credentials
- SSH credentials
- git credentials
- remote pairing keys
- private keys
- user secrets
- encrypted identity state

## 15.2 Encryption Design

A recommended architecture:

```text
Passphrase / trusted device
        |
        v
     Argon2id
        |
        v
 Key Encryption Key
        |
        v
 decrypts Vault Master Key
        |
        v
 per-record / per-session keys
        |
        v
 XChaCha20-Poly1305 encrypted records
```

Alternative platform-supported secure key wrapping can be added, but the portable passphrase path must work independently of a specific host machine.

## 15.3 Secret Handles

Agents should not receive raw secrets unless unavoidable.

Prefer:

```text
agent
  |
  v
secret handle
  |
  v
broker
  |
  v
network/tool operation
```

For example, an MCP server may receive credentials directly from the broker without exposing the value to the model context.

## 15.4 In-Memory Handling

Sensitive material should use:

- short lifetime
- memory zeroization
- locked memory where feasible
- no debug logging
- no plaintext session persistence

---

# 16. Drive Encryption and Data Protection

AiDEX security is application-layer first.

Important directories are encrypted individually or through encrypted containers.

Suggested separation:

```text
public/
runtime/
models/
encrypted/
  vault/
  identity/
  sessions/
  memory/
  private-config/
```

Large public model weights do not need to be encrypted unless the user chooses full-drive confidentiality.

This allows:

- faster model loading
- deduplication
- integrity checks
- encrypted private state without encrypting hundreds of GB unnecessarily

---

# 17. Device Identity

Every AiDEX drive gets its own cryptographic identity.

Example:

```text
Drive Identity
  Ed25519 signing key
  X25519 key agreement key
  UUID
  creation time
  recovery material
```

The identity is stored encrypted.

It is used for:

- remote pairing
- device authorization
- signed session messages
- update verification
- trust decisions
- agent-to-agent identity

---

# 18. Remote Control Architecture

Remote control must be designed as part of AiDEX Core rather than bolted onto the desktop UI.

## 18.1 Pairing

Typical flow:

1. user opens Remote Pairing on desktop
2. AiDEX shows QR code
3. mobile app scans QR
4. devices perform authenticated key agreement
5. phone is added as a trusted controller
6. user selects permission scope

QR payload should contain only bootstrap information, not reusable plaintext credentials.

## 18.2 Session Security

Recommended primitives:

- X25519 key agreement
- Ed25519 identities
- modern AEAD
- replay protection
- session sequence numbers
- per-session forward-secret keys

## 18.3 Transport

AiDEX may negotiate:

1. local direct connection
2. QUIC
3. WebRTC data channel
4. relay-assisted encrypted tunnel

The relay is transport only.

The application message remains encrypted end-to-end.

## 18.4 Remote Permission Scopes

Example:

```yaml
mobile:
  view_status: true
  send_prompts: true
  approve_safe: true
  approve_secrets: false
  approve_destructive: false
  view_terminal: true
  control_processes: true
```

## 18.5 Remote Session View

The phone sees:

```text
Machine: Lab-PC-04
Drive: AiDEX-Neeraj
Workspace: lattice
Agent: AiDEX Native
Model: Gemma 4 Local

Task:
Fix failing desktop voice integration tests

State:
Running

Current action:
bun test apps/desktop

[Pause] [Stop] [Open Terminal] [Approvals]
```

---

# 19. Event Bus

Everything important inside AiDEX should emit structured events.

Examples:

```text
session.started
session.paused
session.completed

agent.started
agent.message
agent.subagent.created
agent.error

tool.requested
tool.started
tool.completed
tool.failed

approval.created
approval.approved
approval.denied

process.started
process.output
process.exited

file.changed
diff.created

model.loaded
model.unloaded
model.fallback

remote.connected
remote.disconnected
```

The same event stream powers:

- desktop UI
- CLI/TUI
- mobile app
- session history
- diagnostics

---

# 20. Durable Session Store

Agent sessions live on the drive.

Each session stores:

- task
- messages
- tool calls
- results
- agent graph
- active model
- checkpoints
- diffs
- process metadata
- approvals
- artifacts
- compressed context
- event log

Recommended architecture:

- SQLite for structured durable state
- content-addressed blob store for large artifacts
- optional vector index for local memory
- append-only event log for recovery

Example:

```text
sessions/
  01J.../
    session.db
    events.log
    artifacts/
    checkpoints/
    diffs/
```

---

# 21. Checkpointing

Checkpoints should be written:

- before destructive operations
- after meaningful implementation phases
- before model switch
- before host shutdown
- before remote takeover
- periodically during long tasks

A checkpoint contains enough information for the harness to reconstruct logical state.

It does not need to snapshot arbitrary external host processes.

---

# 22. Cross-Machine Resume

A major AiDEX feature is moving a task between machines.

Example:

```text
Laptop A
  |
  | agent implements feature
  | checkpoint
  v
unplug drive
  |
  v
Workstation B
  |
  | AiDEX probes host
  | selects stronger local model
  | restores task graph
  v
agent continues
```

Host-dependent process state is re-created.

Portable logical state persists.

---

# 23. Workspace Model

AiDEX should distinguish:

- host workspace
- drive workspace
- cloned workspace
- ephemeral workspace
- sandbox overlay

## 23.1 Host Workspace

AiDEX works directly on a repository already present on the host.

## 23.2 Drive Workspace

The repository itself is stored on the AiDEX drive.

## 23.3 Mirror Workspace

Repository is mirrored between host and drive.

## 23.4 Ephemeral Workspace

Agent gets a temporary sandbox copy.

Changes are reviewed before applying to the actual repository.

---

# 24. Portable Skills

A skill is a reusable instruction/tool package.

Example:

```text
skills/
  release-engineer/
    skill.md
    tools/
    templates/
    tests/
    manifest.yaml
```

Manifest:

```yaml
id: release-engineer
version: 1.2.0
permissions:
  shell: true
  network:
    - github.com
  secrets:
    - github.token
```

Skills are portable across machines.

---

# 25. MCP Support

MCP is a core extensibility mechanism.

AiDEX should support:

- stdio MCP
- local HTTP MCP
- remote HTTP MCP
- authenticated servers
- per-workspace configuration
- capability discovery
- health checks
- automatic lifecycle management

Each MCP server receives a security policy.

An MCP server should not automatically inherit the entire AiDEX environment.

---

# 26. Plugin System

Plugins can extend:

- agent adapters
- models
- tools
- skills
- MCP integrations
- UI panels
- remote commands
- sandbox backends
- authentication providers

Plugin manifest:

```yaml
id: github-plus
version: 1.0.0
entry:
  windows: plugin.exe
  linux: plugin
permissions:
  network:
    - github.com
  secrets:
    - github.token
```

Plugins should be signed or explicitly trusted before activation.

---

# 27. Portable Toolchains

AiDEX may optionally carry developer runtimes.

Example packs:

```text
toolchains/
  bun/
  node/
  python/
  uv/
  rust/
  git/
  ripgrep/
  fd/
  jq/
```

AiDEX should prefer existing host installations when trusted and compatible, but it can fall back to portable bundled versions.

The user can choose:

```text
Host Preferred
Drive Preferred
Drive Only
```

---

# 28. AiDEX Environment Manifest

Each drive includes a high-level declarative manifest.

Example `aidex.yaml`:

```yaml
version: 1

identity:
  name: Neeraj-AiDEX

default_agent: aidex-native
default_model: gemma4-code-q4

agents:
  - aidex-native
  - codex
  - claude-code
  - omp
  - antigravity

models:
  - gemma4-code-q4
  - gemma4-general-q4

skills:
  auto_load:
    - git
    - code-review
    - testing

sandbox:
  default_level: 2

remote:
  enabled: true
  relay: auto

cleanup:
  host_temp: true
  host_logs_created_by_aidex: true
```

This is effectively the portable environment definition.

---

# 29. Proposed Drive Layout

```text
AiDEX/
├── AiDEX.exe
├── aidex
├── aidex.yaml
├── manifests/
│   ├── drive.json
│   ├── checksums.json
│   └── sbom.json
│
├── app/
│   ├── desktop/
│   └── assets/
│
├── runtime/
│   ├── windows-x64/
│   ├── linux-x64/
│   ├── linux-arm64/
│   └── macos-arm64/
│
├── agents/
│   ├── aidex-native/
│   ├── codex/
│   ├── claude-code/
│   ├── omp/
│   ├── antigravity/
│   └── custom/
│
├── models/
│   ├── gemma4/
│   │   ├── code-q4/
│   │   └── general-q4/
│   ├── embeddings/
│   └── registry/
│
├── toolchains/
│   ├── bun/
│   ├── python/
│   ├── git/
│   └── rust/
│
├── skills/
├── plugins/
├── mcp/
├── prompts/
├── policies/
│
├── encrypted/
│   ├── vault/
│   ├── identity/
│   ├── config/
│   ├── sessions/
│   └── memory/
│
├── workspaces/
├── cache/
├── tmp/
└── logs/
```

---

# 30. AiDEX Core Services

The Rust core can be split into logical crates.

```text
crates/
  aidex-core
  aidex-daemon
  aidex-harness
  aidex-agent
  aidex-model
  aidex-runtime
  aidex-sandbox
  aidex-exec
  aidex-policy
  aidex-vault
  aidex-remote
  aidex-mcp
  aidex-skills
  aidex-plugins
  aidex-session
  aidex-storage
  aidex-host
  aidex-events
  aidex-update
  aidex-cleanup
```

---

# 31. Suggested Repository Structure

```text
aidex/
├── apps/
│   ├── desktop/
│   ├── mobile/
│   └── cli/
│
├── crates/
│   ├── core/
│   ├── daemon/
│   ├── harness/
│   ├── runtime/
│   ├── sandbox/
│   ├── exec/
│   ├── policy/
│   ├── vault/
│   ├── remote/
│   ├── models/
│   ├── agents/
│   ├── mcp/
│   ├── skills/
│   ├── plugins/
│   ├── sessions/
│   ├── host/
│   ├── storage/
│   └── cleanup/
│
├── packages/
│   ├── ui/
│   ├── protocol/
│   ├── schemas/
│   └── sdk/
│
├── runtimes/
│   ├── windows/
│   ├── linux/
│   └── macos/
│
├── agents/
│   ├── codex/
│   ├── claude-code/
│   ├── omp/
│   ├── antigravity/
│   └── templates/
│
├── model-registry/
├── skills/
├── mcp/
├── plugins/
├── docs/
├── examples/
├── scripts/
└── tests/
```

---

# 32. Desktop UI Concepts

The main AiDEX session should look like a serious agent workspace.

```text
+---------------------------------------------------------------+
| AiDEX | lattice | Gemma 4 Local | Sandbox L2 | Phone Online  |
+-------------+------------------------------+------------------+
| Files       | Agent                        | Activity         |
|             |                              |                  |
| src/        | Fix the desktop voice layer  | PLAN             |
| packages/   |                              | TOOL CALL        |
| tests/      | [agent response...]          | SHELL            |
|             |                              | DIFF              |
|             |                              | SUBAGENT          |
+-------------+------------------------------+------------------+
| Terminal / Diff / Tests / Approvals                           |
+---------------------------------------------------------------+
```

Important visual objects:

- agent timeline
- task graph
- diff cards
- shell command cards
- model badge
- sandbox badge
- local/cloud badge
- mobile connection badge
- checkpoint timeline

---

# 33. Multi-Agent Orchestration

AiDEX should support parallel agent workers.

Example:

```text
Supervisor
├── frontend-agent
├── backend-agent
├── test-agent
└── reviewer-agent
```

Each subagent can receive:

- isolated context
- file scope
- tool scope
- token budget
- model
- timeout
- sandbox
- branch/worktree

## 33.1 Git Worktree Isolation

For coding tasks, subagents may work in separate git worktrees.

```text
main workspace
  |
  +-- .aidex/worktrees/frontend
  +-- .aidex/worktrees/backend
  +-- .aidex/worktrees/tests
```

The supervisor merges validated output.

---

# 34. Context Engine

The agent should not repeatedly dump the entire repository into context.

AiDEX builds a local project index containing:

- file metadata
- symbols
- git history summaries
- dependency graph
- code embeddings
- test mapping
- documentation
- project rules

Context is selected per task.

Pipeline:

```text
task
 -> query understanding
 -> lexical search
 -> semantic search
 -> graph expansion
 -> reranking
 -> context pack
 -> model
```

---

# 35. Memory

AiDEX has at least three memory scopes.

## 35.1 Session Memory

Only current task/session.

## 35.2 Workspace Memory

Useful facts about a repository.

Examples:

- architecture decisions
- build commands
- coding conventions
- recurring failures
- user preferences for that repository

## 35.3 Portable User Memory

Optional cross-project preferences carried on the drive.

Memory must be editable and inspectable.

It must never silently become an opaque store of secrets.

The memory subsystem should remain pluggable so a separate memory platform can be integrated later without coupling AiDEX to it.

---

# 36. Model Router

The router chooses the best model for a task.

Inputs:

- task type
- privacy level
- network state
- available providers
- local hardware
- latency target
- cost budget
- required context
- tool-use support

Example:

```text
simple summarization -> local small model
code edit            -> Gemma 4 code
huge reasoning task  -> cloud model
secret-heavy task    -> local only
offline              -> local only
```

Users can override routing.

---

# 37. Offline Mode

Offline mode disables network egress except optional local LAN services.

Available features:

- AiDEX native agent
- Gemma 4
- local GGUF models
- local embeddings
- local skills
- local MCP servers
- filesystem
- shell
- local git
- compilers
- tests
- mobile LAN control

This is a complete development mode, not simply a read-only fallback.

---

# 38. Networked Mode

Networked mode enables:

- cloud models
- GitHub/GitLab
- package registries
- online documentation
- remote MCP
- remote mobile access
- external APIs

Each external connection still goes through policy.

---

# 39. Session Migration

AiDEX should allow:

```text
local agent -> cloud agent
cloud agent -> local agent
codex -> AiDEX Native
omp -> AiDEX Native
AiDEX Native -> claude code
```

The system translates the durable session into an adapter-neutral context package.

Exact internal model state cannot be migrated, but task state can.

---

# 40. Authentication

Different services may require different authentication types.

AiDEX supports:

- API keys
- OAuth where usable
- CLI login state
- device login
- SSH keys
- short-lived access tokens

Authentication state should be stored in the encrypted vault whenever technically possible.

Some third-party tools may insist on using host keychains or their own directories. AiDEX should detect and clearly report such exceptions rather than pretending all state is portable.

---

# 41. Host Residue and Cleanup

AiDEX should minimize host residue, but it must describe this accurately.

A normal desktop OS may itself record:

- process execution
- filesystem access
- USB history
- security events
- crash logs
- network metadata
- system telemetry

AiDEX cannot guarantee deletion of evidence controlled by a privileged host OS.

What AiDEX *can* control:

- its temporary files
- extracted binaries
- caches
- named pipes/sockets
- temporary environment files
- temporary auth material it created
- sandbox layers
- runtime directories

AiDEX maintains a **cleanup manifest**.

Example:

```json
{
  "session": "01J...",
  "created": [
    "C:\\Users\\x\\AppData\\Local\\Temp\\aidex-01J"
  ],
  "cleanup_status": "complete"
}
```

The UI should report:

```text
AiDEX-managed residue: cleaned
Host OS telemetry: outside AiDEX control
```

---

# 42. Portable Launch

## Windows

Primary entry:

```text
AiDEX.exe
```

## Linux

Primary entry:

```bash
./aidex
```

## macOS

Primary entry:

```text
AiDEX.app
```

Platform signing and execution restrictions must be handled per OS.

AiDEX should not rely on universal USB autorun.

Optional companion software may provide insertion detection and instant launch for users who deliberately install it.

---

# 43. Update System

AiDEX updates must not corrupt the drive.

Recommended pattern:

```text
current/
staging/
previous/
```

Update process:

1. download update
2. verify signature
3. verify manifest
4. install to staging
5. self-test
6. atomic switch
7. retain rollback version

Separate update channels:

- AiDEX Core
- desktop app
- agent adapters
- model runtimes
- skills
- plugins
- model packs

---

# 44. Supply Chain Security

Every distributed artifact should have:

- hash
- version
- source
- signature where possible

AiDEX should maintain:

```text
SBOM
runtime manifest
plugin manifest
model checksums
adapter checksums
```

Untrusted plugins and binaries require explicit approval.

---

# 45. Observability

AiDEX needs excellent diagnostics because it runs across unknown machines.

Local metrics:

- model load time
- tokens/sec
- time to first token
- GPU offload
- CPU usage
- RAM usage
- VRAM usage
- tool latency
- sandbox denials
- agent retries
- provider failures
- drive I/O
- session recovery

Logs are stored on the drive by default.

Sensitive data must be redacted.

---

# 46. Model Benchmarking

On first launch on a new host, AiDEX can run a short local benchmark.

Example output:

```text
Gemma 4 Code Q4

Load:       4.8 s
Prompt:     72 tok/s
Generate:   28 tok/s
GPU offload: 31 layers
RAM:        7.2 GB
VRAM:       5.1 GB

Recommended: YES
```

The result is cached per host fingerprint.

---

# 47. Remote Notifications

The mobile app may receive notifications for:

- task completed
- task failed
- approval needed
- secret approval needed
- destructive command requested
- agent paused
- local model crashed
- remote session lost
- drive low on space

Push notification infrastructure should carry only minimal metadata.

Sensitive details are fetched through the encrypted session.

---

# 48. Emergency Stop

Both desktop and mobile must have a high-priority stop action.

Emergency Stop:

- freezes new tool execution
- cancels broker approvals
- terminates agent child processes
- closes remote task execution
- locks secret access
- optionally locks the AiDEX vault

---

# 49. Policy Profiles

Built-in profiles:

## Safe

- restricted sandbox
- network allowlist
- approvals for writes outside workspace
- approvals for git push
- approvals for secrets
- approvals for destructive shell

## Balanced

- workspace writes automatic
- safe shell automatic
- remote writes approved

## Autonomous

- broad workspace permissions
- pre-approved tool scopes
- destructive actions still protected by hard rules

## Offline Secure

- local models only
- no Internet
- restricted sandbox
- secrets unavailable unless locally required

---

# 50. Hard Safety Rules

Regardless of agent autonomy, AiDEX should maintain hard barriers for configurable high-risk operations.

Examples:

- disk formatting
- raw drive modification
- credential export
- unrestricted secret dumping
- deleting arbitrary user directories
- modifying the AiDEX vault itself
- disabling the broker
- changing security policy without authorization

---

# 51. Agent Session Protocol

Conceptual request:

```json
{
  "task_id": "tsk_123",
  "workspace": "workspace_abc",
  "agent": "aidex-native",
  "model": "gemma4-code-q4",
  "sandbox": "restricted",
  "input": "Implement the auth refresh flow",
  "permissions": {
    "shell": "brokered",
    "network": "allowlist",
    "secrets": "approval"
  }
}
```

Events are streamed through the central bus.

---

# 52. Internal API Boundary

AiDEX Core should expose a local authenticated API to its frontends.

Example services:

```text
SessionService
AgentService
ModelService
ToolService
ApprovalService
WorkspaceService
VaultService
RemoteService
SandboxService
PluginService
MCPService
SkillService
UpdateService
HostService
```

Desktop, CLI and mobile operate on the same conceptual protocol.

---

# 53. Mobile Remote Protocol

Commands must be idempotent where possible.

Example:

```json
{
  "request_id": "req_xyz",
  "session_id": "ses_123",
  "action": "approval.resolve",
  "payload": {
    "approval_id": "apr_55",
    "decision": "approve_once"
  }
}
```

Signed sequence numbers prevent replay.

---

# 54. Project Indexing

AiDEX can maintain a `.aidex` metadata area on the drive or outside the repository.

```text
index/
  symbols.db
  vectors.db
  git.db
  summaries/
```

The repository does not need to contain AiDEX metadata unless the user chooses to commit project rules.

---

# 55. Project Rules

Workspace-specific rules can be declared:

```yaml
project:
  package_manager: bun
  tests: bun test
  lint: bun lint

agent:
  never_modify:
    - migrations/production
  require_approval:
    - git.push
```

---

# 56. Tool Calling

Tools are typed.

Examples:

```text
fs.read
fs.write
fs.patch
fs.search
shell.exec
git.status
git.diff
git.commit
git.push
process.spawn
http.request
mcp.call
model.invoke
agent.spawn
artifact.create
```

Typed tools make policy enforcement much stronger than unrestricted shell access.

---

# 57. Verification Loop

AiDEX should encourage agents to prove their work.

Default coding flow:

```text
understand
 -> plan
 -> implement
 -> format
 -> lint
 -> test
 -> inspect diff
 -> verifier agent
 -> report
```

The verifier can use a different model from the implementation model.

---

# 58. Background Task Model

AiDEX Core can maintain multiple active tasks while the desktop is minimized.

Tasks remain associated with the drive session.

The mobile app can interact with them remotely.

If the drive is removed, tasks depending on drive state must stop cleanly.

---

# 59. Safe Drive Removal

Before removal AiDEX should:

1. stop new tasks
2. checkpoint agents
3. flush databases
4. close model files
5. close sandbox processes
6. zero temporary secrets
7. run cleanup
8. unmount internal encrypted containers if used
9. mark drive clean

The UI shows:

```text
Safe to remove AiDEX
```

---

# 60. Unexpected Removal Recovery

If the drive is removed unexpectedly:

- child agents lose drive state access
- broker stops secret grants
- daemon terminates managed processes where possible
- mobile session is revoked
- session is marked interrupted

On next launch AiDEX runs recovery and integrity checks.

---

# 61. Host Trust

AiDEX should classify hosts:

```text
Trusted
Known
New
Restricted
Untrusted
```

A new host may default to stricter behavior.

For example:

```text
New host:
- vault auto-lock disabled
- secrets require phone confirmation
- local models allowed
- remote provider tokens require approval
```

---

# 62. Host Fingerprint

AiDEX can remember a privacy-preserving fingerprint based on stable machine characteristics.

It must not depend on an invasive hardware fingerprint.

The fingerprint exists only to recognize previous environment capability profiles and trust decisions.

---

# 63. Phone as Security Factor

The mobile app can optionally act as a second factor.

Examples:

- unlock vault
- approve new host
- approve secret access
- approve remote git push
- revoke stolen drive

A stolen drive remains encrypted.

---

# 64. Lost Drive Response

If a drive is lost:

- vault is passphrase protected
- mobile can revoke remembered pairings
- cloud provider credentials can be rotated independently
- remote relay identity can be revoked
- backups can restore configuration to a new drive

AiDEX should support encrypted recovery bundles.

---

# 65. Backup

Backup should be selective.

Users may back up:

- configs
- skills
- MCP definitions
- plugins
- policies
- sessions
- memory
- vault recovery metadata

Large model files can be excluded because they can be re-imported.

---

# 66. Model Pack Import

A user can add a local model:

```bash
aidex model import model.gguf
```

AiDEX:

1. hashes it
2. identifies metadata
3. checks runtime compatibility
4. benchmarks optionally
5. creates registry entry
6. stores it on drive

---

# 67. Runtime Bundles

AiDEX should not unpack the entire environment onto every host.

Where possible binaries execute from the drive.

When extraction is required, the cleanup manager tracks it.

Portable runtime bundles are content-addressed to prevent unnecessary duplication.

---

# 68. Performance Strategy

USB storage can become a bottleneck, especially for large model weights.

AiDEX should support:

- memory mapping
- read-ahead
- model chunk verification
- optional ephemeral host cache
- encrypted temporary host cache for sensitive material
- model copy-to-fast-disk mode with cleanup
- direct drive mode

User can choose:

```text
Privacy First
Balanced
Performance First
```

---

# 69. Drive Classes

AiDEX can run on:

- USB flash drive
- USB SSD
- portable NVMe
- high-speed removable storage

The UI should measure actual throughput and adapt.

Large local models will perform best on faster storage.

---

# 70. Model Storage Profiles

Example:

## Lite

- smaller Gemma 4 quantization
- embeddings
- core runtime

## Developer

- Gemma 4 coding
- Gemma 4 general
- embeddings
- reranker

## Max

- multiple quantizations
- specialized coding models
- vision/speech models
- larger context models

---

# 71. AiDEX SDK

An SDK should allow external developers to build integrations.

Possible packages:

```text
@aidex/sdk
aidex-sdk-rust
aidex-sdk-python
```

Capabilities:

- define tool
- define skill
- define agent adapter
- subscribe to events
- request approval
- interact with sessions
- register model backend

---

# 72. Agent Adapter SDK

Adapter contract example:

```ts
export interface AgentAdapter {
  id: string
  probe(): Promise<Capabilities>
  start(req: StartRequest): Promise<SessionHandle>
  send(handle: SessionHandle, msg: AgentMessage): Promise<void>
  events(handle: SessionHandle): AsyncIterable<AgentEvent>
  stop(handle: SessionHandle): Promise<void>
}
```

---

# 73. Portable Identity Profiles

Users may maintain multiple profiles on one drive.

Example:

```text
Personal
University
Work
CTF
Open Source
```

Each can have separate:

- secrets
- skills
- models
- MCP servers
- policies
- agent configs
- remote devices

---

# 74. Session Branching

A session can branch from a checkpoint.

```text
checkpoint 4
   |
   +-- try Gemma 4 approach
   |
   +-- try codex approach
```

Results can be compared.

This is useful for:

- model evaluation
- debugging
- alternate implementations

---

# 75. Agent Competition and Verification

AiDEX can ask multiple agents to solve the same problem.

Example:

```text
codex        -> patch A
claude code  -> patch B
Gemma 4      -> patch C
        \       |       /
         verifier agent
              |
          best patch
```

This is an advanced optional workflow rather than the default for every task.

---

# 76. Model Context Compression

Long tasks need durable context management.

AiDEX stores:

- raw event history
- compact summaries
- decision log
- important code references
- unresolved TODOs
- task graph

The active model receives only the relevant compacted view.

---

# 77. Artifact System

Agents exchange typed artifacts.

Examples:

- patch
- file
- test report
- plan
- benchmark
- screenshot
- command output
- research note
- build log

Artifacts have:

- ID
- producer
- timestamp
- type
- hash
- session
- source paths

---

# 78. Local Search

AiDEX should bundle fast local code search.

Core primitives:

- lexical grep
- filename search
- symbol search
- embedding search
- dependency graph traversal
- git history search

Search must work offline.

---

# 79. Git Integration

Built-in git tools:

- status
- diff
- branch
- worktree
- commit
- rebase
- cherry-pick
- push
- pull
- conflict inspection

High-impact remote operations pass through approvals.

---

# 80. Terminal System

The desktop app includes a real terminal connected to the selected workspace.

AiDEX can show which commands were:

- typed by user
- requested by agent
- executed automatically
- remotely approved

This provenance should remain visible.

---

# 81. Session Provenance

Every meaningful action records:

```text
who requested it
which agent
which model
which skill
which tool
which policy allowed it
whether a human approved it
what changed
```

This is important for autonomous coding.

---

# 82. Privacy Modes

## Local Only

No external model or service.

## Hybrid

Local by default, external services when needed.

## Cloud Preferred

Cloud models primary, local fallback.

## Air-Gapped

Networking disabled at broker level.

---

# 83. Sandboxed Browser and GUI Automation

Later AiDEX versions may expose browser or computer-use capabilities.

These should run inside isolated environments where possible and be controlled by the same broker.

GUI automation must not bypass AiDEX security policy.

---

# 84. Remote Desktop Is Not Required

The mobile control path sends structured AiDEX events and commands.

It should not depend on streaming the entire desktop screen.

This makes remote control:

- lighter
- safer
- lower latency
- easier to audit

Optional visual streaming can be added later.

---

# 85. AiDEX Daemon Lifecycle

Typical startup:

```text
launcher
  |
  v
verify drive integrity
  |
  v
start aidexd
  |
  v
host probe
  |
  v
unlock identity/vault
  |
  v
compose runtime
  |
  v
start desktop
  |
  v
restore sessions
```

Shutdown reverses these steps.

---

# 86. Integrity Verification

On launch:

- verify core binary
- verify manifests
- verify critical runtime libraries
- verify model metadata
- verify database consistency

Full model hashes may be lazily verified because large models can be expensive to hash on every boot.

---

# 87. Crash Recovery

After abnormal termination:

```text
Recovering AiDEX

✓ Vault consistent
✓ Session database recovered
✓ 2 stale child processes not found
✓ Sandbox overlay recovered
! Previous task interrupted during test execution

[Resume Task] [Inspect] [Discard]
```

---

# 88. Remote Relay

A hosted or self-hosted relay may provide:

- NAT traversal assistance
- presence
- notification fan-out
- connection rendezvous

The relay should not require access to:

- prompts
- source code
- secrets
- command output
- model messages

Payloads remain end-to-end encrypted.

---

# 89. Optional AiDEX Companion

AiDEX works without a permanent host installation.

An optional tiny companion can add:

- launch on drive insertion
- better OS integration
- privileged sandbox support
- system tray
- faster trusted-host pairing
- local notifications

The core product must not require it.

---

# 90. Security Threat Model

AiDEX should defend against:

- lost drive
- stolen drive
- malicious model output
- prompt injection
- malicious repository instructions
- compromised MCP server
- malicious plugin
- remote session hijacking
- accidental destructive shell command
- secret exfiltration
- untrusted new host
- corrupted model/runtime binary
- agent runaway resource usage

AiDEX cannot fully protect against:

- a host OS controlled by an attacker with administrator/root privileges
- malicious firmware
- hardware keyloggers
- kernel-level malware already present on the host
- physical attacks beyond the protection provided by encryption

The UI and documentation must be explicit about this boundary.

---

# 91. Prompt Injection Defense

AiDEX should distinguish:

- user instructions
- project content
- tool output
- web content
- MCP output
- agent-generated text

External text does not automatically become trusted instructions.

The policy engine can mark sources as untrusted.

---

# 92. Secret Exfiltration Defense

Local models and cloud models should not receive all secrets.

Secrets are scoped to tools.

Example:

```text
GitHub token
  allowed:
    github MCP
    git push tool
  denied:
    model prompt
    arbitrary shell env
```

---

# 93. Plugin Isolation

Untrusted plugins should execute with reduced permissions.

Plugins must declare permissions at install time.

Example:

```text
This plugin requests:
✓ read repository
✓ access network: github.com
✗ access vault
✗ execute arbitrary shell
```

---

# 94. Security Audit Trail

Audit events are append-only within a session.

Examples:

```text
vault.unlocked
secret.granted
approval.created
approval.approved
policy.changed
remote.paired
remote.revoked
sandbox.violation
```

Audit history is stored encrypted.

---

# 95. Advanced Local Inference

Future optimization areas:

- speculative decoding
- prompt caching
- KV-cache persistence
- multi-model routing
- GPU/CPU split
- multi-GPU support
- quantization-aware routing
- model warm pools
- context cache reuse
- model unloading under memory pressure

---

# 96. Local Model Service

The model runtime should run behind an internal service rather than be linked directly into every agent.

Conceptually:

```text
Agent Harness
     |
     v
Model Service
     |
     +--> llama.cpp worker
     +--> embedding worker
     +--> cloud provider worker
```

This makes switching and isolation easier.

---

# 97. Model Concurrency

AiDEX can schedule:

- one large model
- several small models
- embedding worker
- reranker

based on memory pressure.

A resource scheduler prevents all subagents from trying to load large models simultaneously.

---

# 98. Portable Model Cache

Compiled kernels and hardware-specific caches may be stored per host profile.

Example:

```text
cache/hosts/<fingerprint>/models/
```

This speeds later launches without polluting the host.

---

# 99. AiDEX App as the Main Experience

Third-party agents are integrations.

The user should not need to leave AiDEX to use them.

AiDEX provides one consistent interface for:

- model choice
- task creation
- approvals
- diffs
- remote control
- session history
- skills
- MCP
- security
- local models
- logs

The underlying agent can change without changing the user experience.

---

# 100. Hackathon Demonstration Flow

A strong demo proves the entire idea in a few minutes.

## Demo Setup

Machine has no preconfigured AiDEX workspace.

## Demo

1. plug in AiDEX drive
2. launch `AiDEX.exe`
3. unlock drive
4. host capability scan completes
5. open a repository
6. launch AiDEX Native or another agent
7. agent receives coding task
8. local Gemma 4 runs directly from the drive
9. agent executes commands inside the brokered sandbox
10. phone connects to the session
11. sensitive command requires approval
12. approve from phone
13. agent completes tests
14. inspect diff
15. checkpoint session
16. safely close AiDEX
17. cleanup report
18. unplug

Optional second machine:

19. plug same drive into second machine
20. resume the same session

That final step demonstrates the real product thesis.

---

# 101. Hackathon MVP

A one-day or short hackathon build should focus on a narrow but real vertical slice.

## Required

- AiDEX desktop launcher
- drive-relative configuration
- AiDEX Core daemon
- one AiDEX native agent path
- one external agent adapter
- local Gemma 4 / GGUF runtime
- host probe
- basic brokered shell
- encrypted vault
- task session storage
- mobile pairing
- remote status
- one mobile approval
- cleanup manifest

## Stretch

- multi-agent
- stronger sandbox
- model router
- session resume on second machine
- MCP
- skills UI
- multiple third-party adapters
- network broker

---

# 102. Development Phases

## Phase 0: Foundation

- repository
- manifests
- drive-relative path layer
- core daemon
- event protocol
- desktop shell
- CLI shell

## Phase 1: Portable Agent

- workspace
- native agent
- shell tools
- file tools
- session store
- local model

## Phase 2: Security

- vault
- execution broker
- policy
- approvals
- cleanup

## Phase 3: External Agents

- codex
- claude code
- omp
- antigravity
- generic adapter SDK

## Phase 4: Remote

- device identity
- QR pairing
- mobile app
- LAN control
- encrypted remote protocol
- relay

## Phase 5: Advanced Harness

- subagents
- task graph
- checkpoints
- verifier
- context engine
- memory

## Phase 6: Advanced Sandbox

- native kernel-backed restrictions
- copy-on-write workspace
- network broker
- resource limits
- isolated runtimes

## Phase 7: Ecosystem

- plugin SDK
- skill registry
- MCP marketplace/import
- model pack manager
- signed updates

---

# 103. Acceptance Criteria

A version of AiDEX is considered architecturally successful when:

1. The same drive launches on multiple supported machines.
2. Important AiDEX configuration is not tied to a single host.
3. The user can run an AiDEX native coding agent.
4. The user can run at least one third-party CLI agent through AiDEX.
5. A local model stored on the drive can complete a task.
6. Host capability detection changes runtime selection appropriately.
7. The agent can execute tools only through the configured policy boundary.
8. The user can approve a sensitive action from mobile.
9. Session state survives an application restart.
10. A session can be resumed after moving the drive to another machine.
11. Secrets remain encrypted at rest.
12. Cleanup reports what AiDEX removed from the host.
13. AiDEX clearly distinguishes its managed residue from host-controlled telemetry.
14. The core experience works without the optional host companion.

---

# 104. Long-Term Vision

AiDEX should evolve into a portable personal compute layer for AI development.

A developer should be able to carry:

```text
their agents
their models
their memory
their skills
their MCP servers
their security policy
their tools
their sessions
their identity
their workflows
```

in one device.

A host computer becomes interchangeable compute.

The intelligence environment remains the user's.

The end-state experience is:

> Plug in AiDEX on any trusted compatible computer and continue exactly where you left off, with the same agents, same models, same tools, same policy, same encrypted identity and same task state.

AiDEX is therefore not merely "Agent Drive."

It is a **portable agentic development environment and execution runtime**.

---

# 105. One-Line Definition

**AiDEX is a secure portable AI development environment that carries its own agent harness, local models, tools, skills, MCP servers, encrypted state and remote controls on a removable drive, then dynamically composes itself around whatever host machine it is plugged into.**
