# Warposs mediation

This directory is the Warp-side entry point for the multi-CLI mediation work.

## Product boundary

Warp is the product host. It detects and presents CLI agent sessions, owns the host-level
mediation registry, prepares explicit handoffs, and previews the receiving agent's MCP
assembly. Codex and Grok are the approved pilot providers; all CLI agents continue to own their execution,
authentication, sandbox, approvals, and internal memory.

Current status is `implemented` for the application-source Codex/Grok session browser and local
quota overview. The session browser remains default-off; desktop visual acceptance is pending.
The mediation registry, reviewed handoff UI, MCP assembly path, and SSH-target quota observation
remain defined or deferred rather than enabled product behavior.

## Canonical Warp documents

- [Product contract](../../specs/warposs-cli-mediation/PRODUCT.md)
- [Technical contract and P1 slice](../../specs/warposs-cli-mediation/TECH.md)
- [Current P0 status](../../specs/warposs-cli-mediation/STATUS.md)

Product changes and product specifications belong in this Warp repository. External
orchestration documents may link here but must not become Warp build or runtime dependencies.

## Upstream baseline

The P0 comparison was made at Warp commit
`26b7c9cdbd011749480be8ace9b7e8d7d3d5da8c` on 2026-07-19. Relevant upstream capabilities:

- [Third-party CLI agents](https://docs.warp.dev/agent-platform/cli-agents/overview/) provide
  auto-detection, rich input, notifications, code review, metadata, Tab Configs, and Remote
  Control.
- [Codex CLI in Warp](https://docs.warp.dev/agent-platform/cli-agents/codex/) uses native
  notification configuration today; the repository also contains a structured Codex plugin
  design under [`specs/codex-warp-plugin`](../../specs/codex-warp-plugin/TECH.md).
- [Warp MCP](https://docs.warp.dev/agent-platform/capabilities/mcp/) configures local MCP
  servers, while the [CLI MCP reference](https://docs.warp.dev/reference/cli/mcp-servers)
  supports UUID, inline JSON, and file inputs for Warp/Oz agent runs.

These are inputs to mediation, not proof that cross-agent handoff already exists.

## Development drive from Universe

For the Jieyuexing checkout only, Universe remains the default orchestration directory and
Warp is an additional writable product checkout. This convention prevents a `cd`-only handoff
from dropping the Wiki and governance context:

```bash
cd /Users/jieyuexing/jieyuexing-universe
./planet-extensions/warp-mediation-adapter/bin/planet-warp-mediation status
./planet-extensions/warp-mediation-adapter/bin/codex-warp "Continue the Warp mediation P1 slice"

# Preview dual-group tabs data (archived excluded by default)
./planet-extensions/warp-mediation-adapter/bin/planet-warp-mediation sessions-list \
  --prefer-universe-cwd --limit 10
```

The optional local adapter also validates `etech://` view envelopes and drafts
`warp.cli-handoff/v1` payloads. Grok or Codex can invoke the same entry instead of relying on a
Warp-only current directory. Its path and behavior are development orchestration inputs;
Warp must never read them at build or runtime. The relevant human work view is
`planet-wiki/work/active/WARP-MEDIATION/` in Universe.

Other contributors can work directly in a Warp checkout with the two canonical specs above;
Universe is not required.

The fixed mediation control path is CLI-first: register → validate → assemble → draft →
review → place. Universe does not enter Warp runtime, and every Workbench view identity uses
`viewUri=etech://...`; HTTP(S) `contentUrl` values remain browser content only.

## P1A entry

The active pilot slice is local and limited to Codex/Grok:

1. Index and present local Codex and Grok sessions without loading full transcripts.
2. Prepare explicit resume actions without automatic submission.
3. Show independently measured local Codex/Grok quota observations in one visual group.
4. On hover, show local source, remaining percentage, optional reset time, and refresh affordance.
5. Keep SSH quota, arbitrary CLI plugins, automatic provider selection, and reviewed handoff/MCP
   materialization outside this slice.

Start from [`TECH.md`](../../specs/warposs-cli-mediation/TECH.md), preserve the existing dirty
worktree ownership, and keep changes behind the proposed `WarpossCliMediation` feature flag.

## Safety defaults

- Do not write credentials, cookies, authorization headers, transcripts, or rendered MCP config.
- Do not auto-install plugins, spawn agents, submit prompts, approve actions, switch providers,
  commit, or push.
- Do not add or average provider quota percentages, and do not present local quota as SSH quota.
- Do not change cloud managed-MCP server contracts in the P0/P1 slice.
- Keep `viewUri` as `etech://`; accept only HTTPS `contentUrl` as browser content.
- Report `unknown`, `detached`, or `restart_required` rather than inferring success.
