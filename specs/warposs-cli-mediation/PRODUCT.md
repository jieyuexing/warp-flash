# Warposs CLI mediation product contract

Status: `defined`; Codex/Grok session and quota source slices are implemented, with desktop
visual acceptance still pending

Product owner: Warp fork

Evidence baseline: Warp `26b7c9cdbd011749480be8ace9b7e8d7d3d5da8c`, observed 2026-07-19

## Product intent

Warp becomes the local mediation host for multiple CLI coding agents. Codex and Grok are the
approved pilot providers; other providers require a later evidence-backed decision. Warp does
not replace those agents. It gives their sessions a
shared host-level registry, prepares explicit handoffs, and assembles the MCP inputs that a
receiving agent is allowed to use.

The product outcome is continuity across agents without silently merging their memory,
credentials, policies, or execution authority.

## Approved pilot focus

The first product loop is deliberately limited to **Codex + Grok**:

1. discover and resume their local sessions;
2. show their independently measured local quota observations in one visual group;
3. expose honest loading, available, and unavailable states plus manual refresh; and
4. preserve room for a later local/SSH target selector without attributing a local quota to a
   remote host.

“Aggregated quota” means a shared overview of two provider observations. Warp must not add,
average, or otherwise manufacture one combined percentage because the providers use different
limit windows and billing semantics.

## Problem

Upstream Warp already detects supported CLI agents and gives them a toolbelt, rich input,
notifications, code review, metadata, Tab Configs, and Remote Control. Those features are
session-local. They do not yet provide a host-owned contract for:

- identifying and selecting multiple live CLI agent sessions as mediation participants;
- handing a bounded outcome and next action from one agent to another;
- keeping a workbench view identity separate from its browser content URL;
- previewing and materializing a least-privilege MCP set for the receiving session; or
- distinguishing a successfully delivered handoff from an agent merely being detected.

## Users and primary jobs

### Developer operating two CLI agents

The developer can see Codex and Grok sessions in Warp, select a source and target,
review a handoff draft, and place the draft in the target input without sending it
automatically.

### Workbench user sharing a view

The user can hand off an ETECH workbench view using an `etech://` `viewUri`. An optional
`https://` `contentUrl` remains browser content and never becomes the view identity.

### Operator assembling tools

The operator can preview which MCP server references will be available to the target agent,
why each reference is included, and whether a new session or restart is required. Secrets are
resolved only at launch/use time and never copied into the registry or handoff.

## Product principles

1. **Warp mediates; agents execute.** Each CLI agent retains its own authentication,
   sandbox, approval flow, model configuration, and internal session truth.
2. **Handoffs are explicit.** Warp never silently sends prompts, shares memory, or starts an
   agent because another session changed state.
3. **Views and content are different resources.** `viewUri` identifies a software view;
   `contentUrl` identifies browser content that view may display.
4. **MCP is least-privilege and late-bound.** The registry stores references and a redacted
   assembly plan, not credentials or rendered runtime config.
5. **Observed state is not authority.** Plugin and terminal events are evidence about a CLI
   session, not proof that an external task was accepted or completed.
6. **The Warp product is self-contained.** External workspaces may drive development, but
   Warp must build and run without Universe files, paths, contracts, or services.

## Acceptance constants

- **Universe 不进 Warp 运行时。** Universe may drive this checkout during development, but
  Warp must not import, read, execute, or require Universe at build or runtime.
- **CLI-first 固定流程。** The mediation path is register → validate → assemble → draft →
  review → place; it does not gain a silent daemon, message bus, or automatic submit branch.
- **`viewUri=etech://...`.** A Workbench view identity always uses the `etech://` scheme;
  an optional HTTP(S) `contentUrl` remains browser content only.

## Product contracts

### 1. Session registry

Warp owns a process-local mediation registry projected from existing CLI agent detection and
structured plugin events. The initial implementation may be in memory and reconstructable;
it must not persist transcripts or secrets.

A `warp.mediation-session/v1` entry contains at least:

| Field | Meaning |
| --- | --- |
| `mediation_session_id` | Warp-generated opaque ID, stable for the registered lifetime |
| `agent_kind` | Normalized agent kind such as `codex` or `grok` |
| `agent_session_id` | Optional agent-reported ID; never used as the only registry key |
| `pane_ref` | Opaque current-process pane reference; not durable across restarts |
| `workspace_ref` | Optional logical workspace/repository reference |
| `cwd` | Observed working directory, if supplied by the session |
| `status` | `detected`, `active`, `blocked`, `completed`, `failed`, `detached`, or `unknown` |
| `capabilities` | Observed support flags, each carrying known/unknown provenance |
| `mcp_assembly_ref` | Optional reference to a redacted assembly plan |
| `started_at` / `observed_at` | Time-bounded observation metadata |

The registry must preserve `unknown` when a plugin is absent, an event version is unsupported,
or a session detaches. It must not infer `completed` from a pane closing.

### 2. MCP assembly

`warp.mcp-assembly/v1` is a target-specific plan, not a secret container. It records:

- target `mediation_session_id` or requested `agent_kind`;
- ordered MCP server references and their source;
- requested capabilities and user approval state;
- a redacted resolution result (`ready`, `restart_required`, `unresolved`, or `denied`);
- creation/expiry observations; and
- validation errors safe for display.

An already-running third-party CLI process is not assumed to accept new MCP configuration.
If the target adapter cannot attach safely, Warp reports `restart_required` or creates a new
session only after explicit user confirmation. P1 does not modify cloud managed-MCP server
contracts or persist rendered headers, environment variables, commands, or tokens.

### 3. Handoff envelope

The compatibility boundary is `warp.cli-handoff/v1`:

```json
{
  "schema": "warp.cli-handoff/v1",
  "handoff_id": "handoff-opaque-id",
  "from_agent": "codex",
  "to_agent": "claude",
  "outcome": "The product and technical contracts are ready for review.",
  "next_action": "Review the P1 acceptance criteria and identify one blocking gap.",
  "task_ref": "product://warp-mediation",
  "view_envelope": {
    "schema": "etech.workbench-view/v1",
    "viewUri": "etech://workbench/view/warp-mediation",
    "kind": "browser",
    "title": "Warp mediation",
    "observedAt": "2026-07-19T16:00:00Z",
    "contentUrl": "https://example.invalid/warp-mediation"
  },
  "evidence_refs": [
    "repo://warp/specs/warposs-cli-mediation/TECH.md"
  ],
  "created_at": "2026-07-19T16:00:00Z"
}
```

Required handoff fields are `handoff_id`, `from_agent`, `to_agent`, `outcome`,
`next_action`, `view_envelope`, and `created_at`. Optional fields are `task_ref`,
`evidence_refs`, `expires_at`, and `notes`.

The nested `etech.workbench-view/v1` envelope requires `viewUri`, `kind`, `title`, and
`observedAt`. `viewUri` must use `etech://`. `contentUrl`, when present, must use
`https://`. The envelope contains
references and bounded summaries, never complete transcripts, chain-of-thought, cookies,
authorization headers, or original sensitive content.

## User-visible flow

1. Warp detects a supported CLI agent session and shows its observed state.
2. The user chooses **Prepare handoff** from a source session.
3. Warp requires a target agent/session, outcome, next action, and valid view envelope.
4. Warp shows the redacted MCP assembly preview for the target.
5. The user confirms the draft.
6. Warp places a rendered handoff in the target rich-input editor.
7. The user edits or submits it; Warp records only delivery state and references.

There is no automatic step 7.

## Upstream capability comparison

| Area | Upstream baseline | Mediation gap | P1 treatment |
| --- | --- | --- | --- |
| BYO CLI agents | Auto-detection plus toolbelt for Codex, Claude Code, OpenCode, and others | Per-pane enhancement is not a selectable multi-agent registry | Project existing sessions into a host registry |
| Session state | `CLIAgentSessionsModel` tracks pane-scoped status/context | Keyed by current `EntityId`; no stable mediation ID or handoff delivery state | Add opaque mediation IDs and explicit lifecycle/provenance |
| Codex/Claude plugins | Structured notifications enrich status; Codex retains native fallback | Notification protocols do not define cross-agent control | Consume events as observations only |
| Code review/Remote Control | Available for supported CLI agents | Collaboration remains within one CLI session | Reuse UI affordances; do not treat sharing as handoff |
| Local MCP | Warp can configure and run local MCP servers | No common per-handoff plan for arbitrary interactive CLI processes | Add redacted plan/preview and target adapter boundary |
| CLI/Oz MCP | `--mcp` accepts UUID/JSON/file and managed resolution exists in the Agent SDK path | Cloud/Oz resolution is not an interactive CLI mediation bus | Reuse pure parsing where safe; leave cloud contracts unchanged |
| Workbench resources | No upstream `etech://` handoff contract | Browser URL could be mistaken for view identity | Validate and display `viewUri` separately from `contentUrl` |

### 4. External session browser (Codex / Grok tabs)

On Warp open, the product may **asynchronously index** local external agent sessions and present
them as **two tab groups**:

| Group | Source (host defaults) | Hide rule |
| --- | --- | --- |
| **Codex** | `~/.codex/sessions/**/rollout-*.jsonl` | Entries under `~/.codex/archived_sessions` (and any session marked archived by Codex) are **not shown** |
| **Grok** | `~/.grok/sessions/<urlencoded-cwd>/<id>/summary.json` | Entries with an explicit archive marker are hidden; otherwise apply age/limit filters only |

Product rules:

1. Tabs show **index projections only**: id, title, cwd, updated_at, group — never full transcripts.
2. Default filter prefers sessions whose cwd matches the current Warp workspace (or configured project root).
3. Clicking a tab opens or focuses the corresponding agent surface (resume CLI / open pane); it does not auto-run tools.
4. Archived sessions remain accessible only through an explicit “Show archived” action (P2+), never on cold start.
5. This feature is **host UI + local filesystem read**; it does not make Universe a runtime dependency.

### 5. Codex/Grok quota overview

The desktop title bar presents Codex and Grok as one visual group while preserving one
observation per provider:

- the compact state is provider name plus remaining percentage, or an explicit loading /
  unavailable marker;
- hover detail identifies the observation as **local**, shows a parseable reset time when the
  provider supplies one, and exposes the existing click-to-refresh action;
- Codex is queried through its local app-server rate-limit method; Grok is queried through its
  local signed-in CLI billing endpoint;
- tokens and account identifiers are used only by the provider query path and are never placed
  in UI labels, persistence, telemetry, or logs; and
- SSH/remote quotas remain `unknown` until a target-specific adapter actually observes them.

## P1 vertical slices

### P1A — Codex/Grok local pilot

P1A is the active minimum slice:

1. Index local Codex and Grok sessions without reading full transcripts.
2. Render two distinct session groups and the two-provider local quota overview.
3. Prepare resume commands without automatic submission.
4. Refresh provider quota observations independently and keep failure isolated per provider.
5. Keep remote-host quota, automatic provider selection, failover, and arbitrary CLI plugins out
   of scope.

P1A acceptance requires focused parser/state/formatting tests plus a real desktop walkthrough of
the grouped session and quota UI. Automated checks alone do not prove visual acceptance.

### P1B — reviewed handoff (deferred)

P1B remains one local, feature-flagged reviewed handoff path:

1. Project two detected sessions into the process-local mediation registry.
2. Show stable mediation IDs, observed status, and capability provenance.
3. Build and validate one `warp.cli-handoff/v1` draft containing an
   `etech.workbench-view/v1` envelope.
4. Build a redacted MCP assembly preview from already configured local references; do not
   start, install, or reconfigure an MCP server.
5. Insert the reviewed handoff into the target rich-input draft; do not auto-submit.
6. Mark delivery as `drafted` or `placed`, never `accepted`, until the target produces an
   explicit supported acknowledgement in a later slice.

P1B acceptance requires unit tests for schema validation and redaction plus an integration test
with two synthetic CLI sessions. A real desktop walkthrough must verify that the target draft
is editable and not sent automatically.

## Non-goals

- replacing CLI agent authentication, sandboxing, permissions, or model selection;
- merging agent memories, transcripts, prompts, or chain-of-thought;
- building a general agent message bus, background daemon, or remote control plane;
- automatically spawning agents, submitting prompts, approving tools, committing, or pushing;
- automatically choosing or failing over to a provider based on quota;
- combining provider percentages into a synthetic total or showing local quota as SSH quota;
- implementing a full MCP proxy/bridge or changing cloud managed-MCP services in P0/P1;
- using Universe, ETECH Theia, or a local absolute path as a Warp runtime dependency;
- treating an `https://` URL as an ETECH view identity;
- embedding business documents or credentials in Warp configuration or source control; or
- claiming configuration, enablement, runtime activity, or consumer use from this contract.

## Security and privacy

- Treat plugin/OSC payloads, CLI output, MCP metadata, URI fields, and external handoffs as
  untrusted input. Validate schema/version, size, encoding, and allowed schemes.
- Render handoff text as data. Never interpolate it into a shell command.
- Never persist or log MCP secrets, OAuth material, cookies, authorization headers, rendered
  environment variables, or credential-bearing URLs.
- Do not include full transcripts by default. Evidence fields contain references with explicit
  provenance and access checks.
- Keep each agent's approval and deny rules in force. A handoff cannot widen permissions.
- Require an explicit user action before creating/restarting a target session or sending input.
- Expired, detached, or unverifiable session observations fail closed to `unknown`.

## Evidence references

- [Warp third-party CLI agents](https://docs.warp.dev/agent-platform/cli-agents/overview/)
- [Codex CLI in Warp](https://docs.warp.dev/agent-platform/cli-agents/codex/)
- [Warp MCP capability](https://docs.warp.dev/agent-platform/capabilities/mcp/)
- [Warp CLI MCP reference](https://docs.warp.dev/reference/cli/mcp-servers)
- [`CLIAgentSessionsModel`](../../app/src/terminal/cli_agent_sessions/mod.rs)
- [Codex Warp plugin spec](../codex-warp-plugin/TECH.md)
- [Managed MCP CLI resolution spec](../managed-mcp-cli-resolution/TECH.md)
