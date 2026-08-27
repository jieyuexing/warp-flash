# WARP-MEDIATION status

Status: `implemented` — Codex/Grok session browser and local quota application-source slices;
desktop visual acceptance pending
Observed Warp baseline: `26b7c9cdbd011749480be8ace9b7e8d7d3d5da8c` on 2026-07-19

This page is the short, Wiki-ready projection of the Warp-side contract and the implemented
external-session-browser and quota slices. It does not claim that the handoff UI, MCP assembly,
SSH quota observation, deployment, or production runtime consumption are enabled.

## Ten-line Wiki status

1. WARP-MEDIATION P0 合同保留，External session browser 本地源码切片已实现。
2. `WarpossExternalSessionTabs` 是默认关闭、可在 Debug 菜单运行时切换的 feature flag。
3. Warp workspace 建立后异步索引 Codex 与 Grok 本机会话，不阻塞 GUI 启动。
4. Codex 只扫描 `$CODEX_HOME/sessions/**/rollout-*.jsonl`，默认不读取归档根。
5. Grok 只读取 `sessions/*/*/summary.json`，`archived=true` 条目默认隐藏。
6. 两组均优先当前 workspace cwd，按 `updated_at` 降序，每组最多 30 条。
7. tab 只投影 id、title、cwd、updated_at，不加载或显示完整 transcript。
8. Codex 点击只准备 `codex resume <id>`，Grok 点击只复制 id，均不自动提交。
9. 标题栏把 Codex/Grok 放在同一视觉组，但保留各自百分比；hover 显示本机来源、可用的重置时间与刷新提示，不计算虚假合计。
10. Universe 仍不进 Warp 运行时；scoped Cargo 测试覆盖会话索引与额度解析/显示边界，桌面视觉验收仍为 `unknown`。

## External session browser slice

- Core/index/UI: `app/src/external_session_index.rs` and
  `app/src/external_session_index_tests.rs`.
- Feature flag: `crates/warp_features/src/lib.rs` (`WarpossExternalSessionTabs`).
- UI placement: grouped Codex/Grok entries precede the existing live vertical-tab groups and
  disappear completely while the flag is off.
- Local enablement: run a debug OSS build with `cargo run -p warp --bin warp-oss`, open the
  **Debug** menu, and toggle **WarpossExternalSessionTabs**. Keep the vertical-tabs panel open to
  inspect the two groups.
- Validation: `cargo test -p warp --lib external_session_index` and the focused
  `warp_features` flag test pass. Manual desktop visual acceptance remains unverified.

## Local quota overview slice

- Sources: `app/src/codex_rate_limits.rs` and `app/src/grok_rate_limits.rs`.
- UI: two compact title-bar pills form one group; each provider retains its own state and manual
  refresh action.
- Hover detail: identifies the observation as local, shows remaining percentage and a valid reset
  timestamp when available, and never displays tokens or account identifiers.
- Boundary: these values describe local credentials even while the active terminal uses SSH;
  remote-host quota remains `unknown`.
- Validation: `cargo test -p warp --lib rate_limits` passes 15 focused tests. Desktop hover/layout
  inspection remains unverified.

## P0 acceptance

- Canonical files: [PRODUCT.md](PRODUCT.md), [TECH.md](TECH.md), and
  [`docs/warposs/README.md`](../../docs/warposs/README.md).
- Hard constants are explicit: `Universe 不进 Warp 运行时`, `CLI-first 固定流程`, and
  `viewUri=etech://...`.
- The P0 contract slice contains no credentials, runtime code, cloud managed-MCP implementation,
  commit, or push; the external-session-browser application-source slice is tracked separately
  above.
