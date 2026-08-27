# Warposs CLI mediation technical contract

Status: `defined`; Codex/Grok session and quota source slices implemented, desktop visual
acceptance pending

Target slice: P1A local Codex/Grok session and quota pilot; reviewed handoff is P1B and deferred

Evidence baseline: Warp `26b7c9cdbd011749480be8ace9b7e8d7d3d5da8c`, observed 2026-07-19

## Decision

Extend Warp's existing in-process CLI agent session model with a mediation projection and an
explicit handoff draft flow. Reuse current agent detection, structured events, rich input, and
MCP parsing boundaries. Do not introduce a daemon, a second session runtime, or changes to the
cloud managed-MCP service in P0/P1.

This is an application-source design. It does not define deployment assembly, configuration,
enablement, or observed runtime use.

The active pilot reuses the existing external-session index and provider quota models for Codex
and Grok. It does not introduce a provider plugin registry. The older Codex-to-Claude handoff
design remains a defined later slice and does not expand the current implementation scope.

## Decision owner and constraints

Warp owns the product implementation and its internal data model. External orchestration can
request work in the Warp checkout, but it does not define Warp runtime behavior.

Hard constraints:

- CLI agents remain independent child processes with their own auth and policy.
- Current upstream Warp behavior must continue when mediation is disabled.
- Handoff review and submission are separate user actions.
- `etech://` identifies a workbench view; HTTP(S) identifies browser content only.
- Secrets are late-bound and absent from registries, handoffs, telemetry, and logs.
- P0/P1 do not modify cloud managed-MCP schemas or server behavior.

## Current evidence

At the baseline revision:

- [`CLIAgentSessionsModel`](../../app/src/terminal/cli_agent_sessions/mod.rs) is a singleton
  whose `sessions` map is keyed by pane `EntityId` and holds agent, status, context, input
  state, plugin version, and remote-host observations.
- [`warp://cli-agent` v1 events](../../app/src/terminal/cli_agent_sessions/event/mod.rs) carry
  session/status observations over structured OSC 777. They do not provide a host command
  channel or delivery acknowledgement.
- Agent-specific plugin managers live under
  [`plugin_manager`](../../app/src/terminal/cli_agent_sessions/plugin_manager/). Their job is
  plugin install/update and rich notification support, not mediation.
- [`MCPSpec`](../../crates/warp_cli/src/mcp.rs) parses UUID, inline JSON, and file inputs.
  [`agent_sdk`](../../app/src/ai/agent_sdk/) resolves those specs for Warp/Oz harness paths.
- The Codex plugin design keeps structured status distinct from native fallback, which confirms
  that listener presence alone is not a trustworthy capability signal.

## Upstream gap matrix

| Upstream surface | What exists at this baseline | Gap to the mediation contract | Planned reuse |
| --- | --- | --- | --- |
| BYO CLI agents | Warp detects supported CLI processes and `CLIAgentSessionsModel` projects pane-scoped status/context. | There is no host-owned mediation identity, explicit source/target selection, handoff lifecycle, or shared least-privilege assembly plan. | P1 projects existing observations into an in-process registry; it does not add a second process/session truth. |
| Codex plugin | Structured OSC 777 events provide rich status, with native OSC 9 retained as a lower-trust fallback. | Status observation is not a cross-agent command channel, delivery acknowledgement, or permission grant. | P1 consumes trusted structured events as provenance and uses existing rich input only to place an editable draft. |
| `managed-mcp-cli-resolution` | The Agent SDK resolves `agent run --mcp` UUIDs local-first, falls back to managed resolution, and materializes ephemeral config without persisting rendered secrets. | That launch-time Warp/Oz path is not a product-side plan for arbitrary already-running external CLI sessions. | P1 previews redacted references only; P2 may reuse parsing/materialization behind agent adapters without changing cloud schemas or persisting resolved config. |

The gap is therefore coordination and explicit transfer, not basic CLI detection, Codex event
parsing, or managed MCP lookup. P0 does not add product runtime code.

## Minimum load-bearing path

```text
terminal command detection / structured plugin event
                    |
                    v
          CLIAgentSessionsModel        MCP settings / explicit refs
                    |                            |
                    v                            v
          MediationRegistry  <----  redacted MediationMcpPlan
                    |
          reviewed HandoffDraft
                    |
                    v
        target session rich-input draft
                    |
              explicit user send
```

Correctness depends on four single-owner facts:

1. The CLI process owns its internal session and execution result.
2. `CLIAgentSessionsModel` owns current pane-scoped observations.
3. `MediationRegistry` owns host-generated mediation IDs and handoff delivery state.
4. The MCP subsystem owns configuration/resolution; mediation stores only references and a
   redacted plan.

`viewUri` remains an externally defined workbench identity. Warp validates and transports it;
it does not infer that a browser URL or local file is the same view.

## Options considered

| Option | Quality and coupling | Migration/operations | Decision |
| --- | --- | --- | --- |
| A. Project the existing in-process session model | Reuses detection, UI, plugin events, and current lifecycle; narrow blast radius | Feature-flagged and fully removable | **Choose for P1** |
| B. Add a local mediation daemon/MCP control server | Could outlive Warp and expose a general API, but duplicates session truth and creates auth/lifecycle work | New process, protocol, install, cleanup, and credential boundary | Reject for P0/P1 |
| C. Keep detection-only support | Zero implementation cost | No explicit handoff, registry identity, or MCP plan | Does not meet the product goal |

Option A is intentionally limited to one Warp process. A future cross-process requirement must
first prove that reconstructable state and existing session sharing cannot satisfy it.

## Component design

### Mediation registry

Add the smallest internal module adjacent to `terminal/cli_agent_sessions`; do not create a new
crate for P1.

Candidate types:

```rust
struct MediationSessionId(String);

struct MediationSession {
    schema: &'static str, // warp.mediation-session/v1
    mediation_session_id: MediationSessionId,
    agent: CLIAgent,
    agent_session_id: Option<String>,
    pane_ref: EntityId,
    workspace_ref: Option<String>,
    cwd: Option<String>,
    status: MediationStatus,
    capabilities: MediationCapabilities,
    mcp_assembly_ref: Option<String>,
    started_at: SystemTime,
    observed_at: SystemTime,
}
```

The registry subscribes to existing `CLIAgentSessionsModelEvent` events. It does not parse
terminal output a second time. IDs are random/opaque, never derived from cwd, prompt text, user
identity, or credential material. `EntityId` stays an internal current-process reference.

Capability values use a tri-state (`supported`, `unsupported`, `unknown`) with provenance such
as command detection, rich plugin, or static adapter support. `listener.is_some()` is not a
capability proof.

P1 keeps the registry in memory. If later persistence is authorized, store only minimal
metadata needed to display `detached` history and reconstruct it; never store transcripts,
prompts, responses, MCP rendered config, or secrets.

### Handoff draft

Represent a handoff as typed data matching `warp.cli-handoff/v1`. Validation occurs before UI
rendering and again before placement in the target input.

Required invariants:

- source and target agents are non-empty and different unless an explicit self-handoff feature
  is added later;
- `view_envelope.viewUri` uses `etech://`;
- optional `contentUrl` uses HTTPS and contains no userinfo;
- timestamp and expiry fields parse and an expired handoff is rejected;
- text and collection sizes are bounded;
- secret-like fields and credential-bearing URLs are rejected;
- unknown top-level schema versions fail closed; and
- handoff content is rendered/escaped as input text, never executed as a command.

Delivery states are `drafted`, `placed`, `sent_by_user`, `acknowledged`, `expired`, and
`failed`. P1 can prove only `drafted` and `placed`. It must not infer `sent_by_user` from a
status transition or `acknowledged` from terminal output.

### Workbench view envelope

The nested `etech.workbench-view/v1` shape is:

```text
required: viewUri, kind, title, observedAt
optional: resourceType, contentUrl, logicalRef, localMirrorRef,
          localMirrorSha256, generation, active
```

Routing rules:

- `viewUri` is the primary reference shown and handed to agents.
- `contentUrl` may be offered to the browser surface but cannot replace `viewUri`.
- P1 validates and displays the envelope; it does not implement an ETECH view resolver.
- Absence of `contentUrl` is valid. An inaccessible `contentUrl` does not invalidate the view
  identity, but it must be reported separately if navigation is attempted.

### MCP assembly plan

Introduce a pure, redacted `warp.mcp-assembly/v1` plan before any agent-specific launch
materialization:

```rust
struct MediationMcpPlan {
    assembly_id: String,
    target_agent: CLIAgent,
    target_session_id: Option<MediationSessionId>,
    server_refs: Vec<McpServerRef>,
    requested_capabilities: Vec<String>,
    approval: ApprovalState,
    status: AssemblyStatus,
    created_at: SystemTime,
    expires_at: Option<SystemTime>,
}
```

`McpServerRef` contains an opaque ID/name and source category only. It must not contain a
resolved command, environment value, header, OAuth token, cookie, or URL with userinfo.

P1 produces a preview from already configured references and reports one of:

- `ready`: the target adapter can materialize it for a newly launched session;
- `restart_required`: an active CLI cannot safely accept a new config;
- `unresolved`: a reference or required binding is unavailable; or
- `denied`: policy/user approval rejects the plan.

P1 does not start MCP servers or mutate Codex/Claude configuration. A later launch slice may
reuse the existing parsing and secret-application machinery, but it must keep local-first
behavior and must not route interactive CLI mediation through the cloud managed-MCP mutation
by default.

### Agent adapter boundary

Agent-specific behavior is typed behind a narrow internal adapter:

```rust
trait CliMediationAdapter {
    fn observed_capabilities(&self, session: &CLIAgentSession) -> MediationCapabilities;
    fn render_handoff(&self, handoff: &ValidatedHandoff) -> String;
    fn mcp_attach_mode(&self, session: &CLIAgentSession) -> McpAttachMode;
}
```

P1 adapters for Codex and Claude only render a reviewed handoff into the existing rich-input
draft and report MCP attach mode. They do not install plugins, write user config, spawn a
process, or submit input.

## State and time axis

| Event | Registry effect | Handoff/MCP effect |
| --- | --- | --- |
| Command detected | `detected`; capabilities mostly `unknown` | No automatic draft or assembly |
| Rich `session_start` | Update agent session ID/context; mark supported observations | Still no automatic handoff |
| Prompt submitted | `active` | A user-submitted placed draft may become `sent_by_user` only with a direct input event |
| Permission/question | `blocked` | Does not imply handoff failure |
| Stop/stop failure | `completed`/`failed` when supported by trusted event | Does not auto-select a new agent |
| Pane closes/process disappears | `detached` | Pending drafts remain local and expire; never mark completed |
| Unsupported event/version | Preserve prior fact, mark affected capability/state `unknown` | Fail closed and show incompatibility |
| Warp restarts | Reconstruct live sessions; old IDs are detached unless a future durable mapping proves identity | P1 drafts/plans are not silently replayed |

## Security boundaries

### Trust edges

- CLI agent/plugin to Warp: untrusted event data; schema/version/size validation required.
- Warp to CLI input: data-only placement; no shell interpolation or automatic submit.
- Warp to MCP runtime: references become config only after explicit approval and target-specific
  validation.
- Workbench/browser to Warp: URI and metadata are untrusted; do not fetch during validation.
- External development drivers to Warp checkout: build-time convenience only; no runtime import.

### Data minimization

Allowed in registry/handoff/plan:

- opaque IDs, agent kind, logical refs, bounded outcome/next action, status/provenance, safe MCP
  reference names, timestamps, and validated URIs.

Forbidden:

- API keys, OAuth material, cookies, authorization headers, passwords, rendered environment
  values, full transcripts, chain-of-thought, raw business documents, or credential-bearing
  URLs.

Telemetry must record schema/version, agent kind, transition/result category, and latency only.
Do not record handoff text, URI query strings, cwd, prompt/response bodies, or MCP config.

## Delivery slices and acceptance

| Slice | Scope | Acceptance | State |
| --- | --- | --- | --- |
| P0 — contract and navigation | Product/technical contracts, security/non-goals, upstream gap matrix, Universe development-drive navigation, and status page. | UTF-8 files exist; they explicitly state `Universe 不进 Warp 运行时`, `CLI-first 固定流程`, and `viewUri=etech://...`; no product/runtime or cloud managed-MCP code changes. | `defined` in this change |
| P1A — Codex/Grok local pilot | Local session index/resume projection plus an independently measured two-provider quota overview. | Parser/state/formatting tests pass; a desktop walkthrough must still prove grouped sessions, compact labels, hover details, and manual refresh. | `implemented`; visual acceptance `unknown` |
| P1B — local reviewed handoff | Feature-flagged in-process registry for one source and target session, typed handoff validation, redacted MCP preview, and editable rich-input placement. | Unit/schema/redaction tests, two synthetic sessions, and a desktop walkthrough prove the draft is editable and never auto-submitted; detached/unknown remain honest. | `planned` |
| P2 — approved MCP materialization | Agent-specific launch/restart adapters materialize an approved least-privilege MCP set for a new or explicitly restarted session and record a bounded receipt. | Real Codex/Claude launch checks prove selected tools only, no secret persistence/logging, explicit restart consent, and no transcript copying; cloud contract changes remain a separate decision. | `deferred` |

The fixed product flow remains CLI-first: register → validate → assemble → draft → review →
place. `viewUri` must satisfy `viewUri=etech://...`; `contentUrl` is never promoted to view
identity. Universe is a development driver only and is not a Warp runtime dependency.

## P1B handoff implementation slice

Feature flag: proposed `WarpossCliMediation`, dogfood/default-off.

Implementation checkpoints:

1. Add typed session/handoff/MCP-plan models and validators with no UI wiring.
2. Add registry projection from `CLIAgentSessionsModelEvent` and expose a read-only list to the
   current process.
3. Add a minimal **Prepare handoff** action for one Codex and one Claude session.
4. Add envelope form/import, redacted MCP preview, and target rich-input placement.
5. Add status/error UI for `unknown`, `detached`, `expired`, and `restart_required`.
6. Validate with synthetic integration events and one real desktop walkthrough.

No checkpoint changes plugin install/update behavior, `AgentDriver` cloud behavior, managed-MCP
server calls, or persistent agent configuration.

## Acceptance and planted violations

### Automated

- Two synthetic pane sessions receive distinct mediation IDs and remain addressable after status
  updates.
- A duplicate agent-reported session ID does not collide or overwrite another pane.
- Closing a pane yields `detached`, not `completed`.
- Codex native fallback leaves rich status capability `unknown`/unsupported as appropriate.
- A valid `etech://` envelope with optional HTTPS content is accepted.
- A handoff with `https://` as `viewUri` is rejected.
- A `contentUrl` using `file://`, containing userinfo, or carrying a secret-like field is rejected.
- An MCP plan serializer cannot emit env/header/token values; a planted secret causes the test to
  fail.
- Placing a draft changes the target editor buffer but does not trigger terminal input/send.
- Unsupported schema versions and expired handoffs fail closed.

### Manual desktop

1. Start one Codex and one Claude CLI session in separate Warp panes.
2. Confirm both appear with distinct IDs and honest capability/status labels.
3. Prepare a handoff from Codex to Claude with an `etech://` view and optional HTTPS content.
4. Confirm the MCP preview shows references/status only.
5. Place the draft in Claude rich input and verify it remains editable and unsent.
6. Close the source pane and verify its status becomes detached without altering the target.

### Validation boundary

Passing these checks proves the local application-source slice only. It does not prove plugin
installation, agent authentication, MCP connectivity, cloud resolution, external workbench
navigation, deployment, or production use.

## Failure and recovery

- Registry projection failure: disable the feature flag; existing CLI session UI remains the
  source of current observations.
- Handoff validation failure: retain the local editable draft, show field-level errors, and do
  not place or send it.
- Target detaches before placement: mark delivery failed and require explicit reselection.
- MCP reference cannot resolve: report `unresolved`; do not drop the server silently and do not
  fall back to a broader set.
- Plugin protocol mismatch: retain detection-only behavior and mark rich capabilities unknown.
- Warp crash/restart: do not replay unsent drafts or restore secrets; reconstruct live sessions.

## Rollback and retirement

P1 is removable by disabling `WarpossCliMediation` and deleting the projection/action code.
Because P1 persists no mediation records or secret-bearing config, rollback requires no data
migration. Retire the feature if the upstream CLI session model gains an equivalent canonical
registry/handoff contract; migrate consumers to that single truth rather than maintaining two
registries.

## Codex/Grok quota overview

### Ownership and sources

- `CodexRateLimitsModel` owns the current process-local Codex quota observation obtained from
  `codex app-server`; it does not own Codex account policy or billing truth.
- `GrokRateLimitsModel` owns the current process-local Grok quota observation obtained from the
  signed-in CLI billing endpoint; it does not own Grok account policy or billing truth.
- Workspace UI owns only the grouped projection and refresh actions. It does not persist either
  observation or merge their percentages.

Both observations describe **local credentials**. An active SSH terminal does not change their
source. Remote quota remains `unknown` until a future SSH-target adapter performs an explicit
remote observation.

### Display and failure behavior

| State | Compact label | Hover detail | Refresh behavior |
| --- | --- | --- | --- |
| `loading` | `<provider> …` | local provider quota is loading | duplicate refresh is ignored while in flight |
| `available` | `<provider> <remaining>%` | local source, remaining percentage, optional reset time, refresh affordance | click refreshes that provider only |
| `unavailable` | `<provider> --` | local provider quota unavailable, click to retry | retry remains isolated to that provider |

The UI never reports a sum, average, “best provider,” automatic switch, or failover recommendation.
Provider errors do not erase the other provider's latest state. Reset timestamps are display
metadata only and do not schedule work or claim the provider will replenish exactly then.

### Quota acceptance

- Codex chooses the most constrained active rate-limit window and clamps invalid percentages.
- Grok supports the current percentage and the observed legacy used/limit fallback and clamps
  invalid percentages.
- Valid provider reset timestamps produce local-time hover detail; invalid timestamps are omitted
  without discarding a valid remaining percentage.
- Loading, available, and unavailable states always have a visible compact and detailed label.
- No credential, account identifier, or raw provider response appears in the label.
- Desktop visual acceptance is required before calling the UI consumed or release-ready.

## External session index (Codex / Grok tab groups)

### Goal

When Warp starts (or the mediation feature flag enables), scan local disk session stores and
render **two groups of tabs**: Codex and Grok. **Archived sessions are excluded** from the
default projection.

### Data sources

| Agent | Active root | Archived root / rule | Entry |
| --- | --- | --- | --- |
| Codex | `$CODEX_HOME/sessions` (default `~/.codex/sessions`) | `$CODEX_HOME/archived_sessions` — **path membership = archived** | `rollout-*.jsonl`; identity from first-line `session_meta.payload.id`; title from best-effort first user text or id short form; cwd from `payload.cwd` |
| Grok | `~/.grok/sessions` | No first-class archive tree observed; hide if future `archived: true` in `summary.json` / sibling marker; optional max-age | `sessions/<urlencoded-cwd>/<uuid>/summary.json`; title from `session_summary` or `generated_title`; cwd from decoded parent path or `git_root_dir` |

### Projection type

```rust
struct ExternalSessionTab {
    group: ExternalSessionGroup, // Codex | Grok
    session_id: String,
    title: String,
    cwd: Option<PathBuf>,
    updated_at: SystemTime,
    archived: bool, // always false in default UI list
    source_path: PathBuf, // for open/resume only; not shown raw if sensitive
}
```

### Algorithm (default UI)

1. Enumerate Codex **active** tree only; never merge archived root into the primary list.
2. Enumerate Grok `summary.json` under configured workspace-encoded dirs (prefer current workspace cwd encode).
3. Drop any entry with `archived == true` or path under an archive root.
4. Optionally keep only cwd matches against Warp workspace roots; always apply `limit_per_group` (default 30), sorted by `updated_at` desc.
5. Render two tab strips / grouped side lists: **Codex** | **Grok**.
6. On click: for Codex, offer `codex resume <id>` in a pane or existing resume UI; for Grok, open/focus Grok CLI/TUI resume if available, else show path + copy id.

### Privacy

- Index fields only; do not load full `chat_history.jsonl` / entire rollout into memory for the tab list.
- Do not write session bodies into the Warp repo or cloud.
- Do not surface cookies, tokens, or tool secrets from session files.

### Universe orchestration helper

Development may call Universe adapter `mediation.sessions.list` to validate the same filter rules
before Warp UI lands. Warp must re-implement scanning in-process and must not shell out to Universe
at runtime.

### Acceptance (feature)

- Cold start list never includes paths under `~/.codex/archived_sessions`.
- Two groups are visually distinct.
- At most `limit_per_group` tabs per group.
- Clicking does not auto-submit prompts.
- Feature flag off restores prior UI with zero leftover tabs.

## P1 exclusions

- no new daemon, socket protocol, HTTP API, or MCP server surface;
- no automatic agent spawn, prompt submit, approval, retry, or failover;
- no synthetic combined quota, automatic provider selection, or remote quota inference;
- no managed-MCP server/schema changes;
- no cross-device or cloud mediation history;
- no persistent transcripts or invisible shared memory;
- no ETECH view renderer/resolver implementation; and
- no dependency on Universe at build or runtime.

## Evidence references

- [Product contract](PRODUCT.md)
- [Warp third-party CLI agent overview](https://docs.warp.dev/agent-platform/cli-agents/overview/)
- [Codex CLI in Warp](https://docs.warp.dev/agent-platform/cli-agents/codex/)
- [Warp MCP capability](https://docs.warp.dev/agent-platform/capabilities/mcp/)
- [Warp CLI MCP reference](https://docs.warp.dev/reference/cli/mcp-servers)
- [`CLIAgentSessionsModel`](../../app/src/terminal/cli_agent_sessions/mod.rs)
- [CLI agent event v1](../../app/src/terminal/cli_agent_sessions/event/v1.rs)
- [Codex plugin design](../codex-warp-plugin/TECH.md)
- [Managed MCP CLI resolution](../managed-mcp-cli-resolution/TECH.md)
