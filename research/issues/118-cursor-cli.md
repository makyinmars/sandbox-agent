# Issue #118 Research: Cursor CLI (`cursor-agent`) Support

Date: 2026-02-07
Issue: https://github.com/rivet-dev/sandbox-agent/issues/118

## Issue Summary

- Issue `#118` requests adding Cursor CLI as a supported agent option.
- Current issue state is `open`.
- Issue body asks for Cursor CLI support as a sandboxed option.

## Sources Reviewed

- Cursor CLI overview: https://cursor.com/docs/cli/overview
- Cursor CLI installation: https://cursor.com/docs/cli/installation
- Cursor CLI using/headless: https://cursor.com/docs/cli/using
- Cursor CLI headless mode: https://cursor.com/docs/cli/headless
- Cursor CLI parameters: https://cursor.com/docs/cli/reference/parameters
- Cursor CLI output format: https://cursor.com/docs/cli/reference/output-format
- Cursor CLI permissions: https://cursor.com/docs/cli/reference/permissions
- Cursor CLI authentication: https://cursor.com/docs/cli/reference/authentication
- Cursor CLI configuration: https://cursor.com/docs/cli/reference/configuration
- Cursor CLI MCP: https://cursor.com/docs/cli/mcp
- Cursor changelog (mode flags): https://cursor.com/changelog/cli-agent-mode

Notes:
- In this environment, direct shell DNS resolution for `cursor.com` failed, so findings are from official docs discovered via web indexing.
- The indexed snippets are recent but not guaranteed to be same-day. Validate final command behavior against a live Cursor CLI binary during implementation.

## Cursor CLI Findings (Relevant to Integration)

- CLI command in docs examples is `cursor-agent`.
- Installation docs use install scripts:
  - macOS/Linux: `curl https://cursor.com/install -fsS | bash`
  - Windows PowerShell: `irm 'https://cursor.com/install?win32=true' | iex`
- Non-interactive usage is via `-p`/`--print`.
- Output formats include `--output-format text|json|stream-json`.
- Session controls include list/resume commands and `--resume [chat-id]`.
- Authentication supports:
  - OAuth/browser login flow (`login`, `logout`, `status`)
  - API key via `CURSOR_API_KEY` or `--api-key`
- Permission behavior includes:
  - `--force` (auto-approve)
  - granular allow/deny config patterns in CLI config
- MCP subcommands are documented (`mcp list`, `mcp list-tools`, `mcp login`; also enable/disable per MCP docs).
- Cursor changelog indicates mode support via `--mode` (`agent`, `plan`, `ask`) as of 2026-01-16.

## Current Agent Wiring in This Repo

### Core agent-manager wiring

- Agent enum and ID parsing are hardcoded in `server/packages/agent-management/src/agents.rs:19`, `server/packages/agent-management/src/agents.rs:28`, `server/packages/agent-management/src/agents.rs:38`, `server/packages/agent-management/src/agents.rs:48`.
- Install dispatch is hardcoded in `server/packages/agent-management/src/agents.rs:126`.
- Spawn command composition is hardcoded in:
  - `server/packages/agent-management/src/agents.rs:199`
  - `server/packages/agent-management/src/agents.rs:553`
- Agent-specific install functions exist for each supported agent:
  - `server/packages/agent-management/src/agents.rs:1213`
  - `server/packages/agent-management/src/agents.rs:1246`
  - `server/packages/agent-management/src/agents.rs:1274`
  - `server/packages/agent-management/src/agents.rs:1304`
- Session/result extraction currently branches by agent in:
  - `server/packages/agent-management/src/agents.rs:881`
  - `server/packages/agent-management/src/agents.rs:949`

### Router and API behavior

- Agent list is hardcoded in `server/packages/sandbox-agent/src/router.rs:4508`.
- Capabilities map is hardcoded in `server/packages/sandbox-agent/src/router.rs:4531`.
- Agent mode support is hardcoded in `server/packages/sandbox-agent/src/router.rs:4649`.
- Mode normalization is hardcoded in `server/packages/sandbox-agent/src/router.rs:4821`.
- Permission mode normalization is hardcoded in `server/packages/sandbox-agent/src/router.rs:4872`.
- Model-fetch routing is hardcoded in `server/packages/sandbox-agent/src/router.rs:1796`.
- Credential env injection into spawned agents currently only maps Anthropic/OpenAI vars in `server/packages/sandbox-agent/src/router.rs:4965`.
- Streaming line parsing is hardcoded by agent converter in `server/packages/sandbox-agent/src/router.rs:5667`.

### Test and compatibility wiring

- Agent auto-detect and `SANDBOX_TEST_AGENTS=all` list are hardcoded in `server/packages/agent-management/src/testing.rs:61` and `server/packages/agent-management/src/testing.rs:300`.
- OpenCode compatibility agent list is hardcoded in `server/packages/sandbox-agent/src/opencode_compat.rs:607`.
- Credential-agent enum for CLI extraction is hardcoded in `server/packages/sandbox-agent/src/cli.rs:967`.
- Agent-management integration tests use fixed agent arrays in `server/packages/sandbox-agent/tests/agent-management/agents.rs:39`.
- Test permission-mode assumptions are hardcoded in:
  - `server/packages/sandbox-agent/tests/common/http.rs:179`
  - `server/packages/sandbox-agent/tests/common/mod.rs:183`

### Schema/conversion wiring

- Extracted schema set currently includes only `amp|claude|codex|opencode`:
  - `resources/agent-schemas/src/index.ts:12`
  - `server/packages/extracted-agent-schemas/build.rs:9`
  - `server/packages/extracted-agent-schemas/src/lib.rs:9`
- Universal converter modules currently include only four real agents:
  - `server/packages/universal-agent-schema/src/agents/mod.rs:1`
  - `server/packages/universal-agent-schema/src/lib.rs:6`

## Recommended Integration Shape

### Phase 1: Minimal functional Cursor agent

- Add `AgentId::Cursor` through all core enums/lists:
  - `agent-management`, `router`, `opencode_compat`, test lists.
- Use subprocess integration model first (similar to Claude/Amp path) with non-interactive mode:
  - `cursor-agent --print --output-format stream-json <prompt>`
- Add resume wiring:
  - pass `--resume <native_session_id>` when session exists.
- Add permission mapping:
  - `permissionMode=bypass` -> `--force`
  - keep `default` conservative (no force).
- Add credential env mapping:
  - set `CURSOR_API_KEY` from credentials when available (or from explicit env pass-through).

### Phase 2: Universal event conversion

- Add Cursor schema extraction and generated Rust types (no handwritten types).
- Add `convert_cursor` module in universal schema package.
- Extend `parse_agent_line` and session ID/result extraction logic to Cursor-native event shape.
- Ensure parse failures emit `agent.unparsed` and fail tests as required by project rules.

### Phase 3: Models, modes, capabilities

- Add `agent_modes_for` and `normalize_agent_mode` mapping for Cursor modes.
- Decide `agent_capabilities_for(AgentId::Cursor)` based on verified stream output:
  - permissions/questions/tool calls/file changes/reasoning flags must reflect actual behavior.
- Add model listing path:
  - either API-backed fetch, CLI-backed query, or conservative static/fallback list with explicit limitations.

### Phase 4: Docs + tests parity

- Update CLI docs and endpoint parity docs:
  - `docs/cli.mdx` (agent filters/examples)
  - `CLAUDE.md` if endpoint/CLI command sets change
  - `docs/conversion.md` and `docs/session-transcript-schema.mdx` if schema/conversion behavior changes
- Add/update route tests and snapshots for:
  - `/v1/agents`
  - `/v1/agents/{agent}/install`
  - `/v1/agents/{agent}/modes`
  - `/v1/agents/{agent}/models`
  - session/message/events SSE flows

## Risks and Open Questions

- Command name drift: docs show `cursor-agent`; some ecosystem references mention `agent`. Implementation should detect both binary names.
- Installer strategy: existing agents use deterministic binary downloads, while Cursor docs primarily describe install scripts. Need a reproducible install path for sandbox environments.
- Event schema stability: Cursor `stream-json` output must be validated for required universal event mapping (`item.started` -> `item.delta` -> `item.completed` semantics).
- Credential provider mapping: current extraction pipeline is focused on Anthropic/OpenAI plus generic providers; Cursor-specific auth discovery may need dedicated extractor paths.

## Practical Next Step

- Implement a minimal `AgentId::Cursor` subprocess path first (spawn + parse passthrough + resume + force mapping), then iterate on schema-accurate converter + model/mode fidelity once real event captures are in place.
