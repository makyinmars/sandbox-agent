# Issue #58 Investigation: Update Model/Variant Mid-Session

**Issue**: feat: allow updating model/variant mid-session (#58)

**Summary (from issue)**
- Model/variant only apply at session creation today; UI edits after start have no effect.
- Request: add a supported way to change model/variant for an active session (e.g., `PATCH /v1/sessions/{id}`).
- Wire Inspector UI to apply changes to the active session and reflect applied values.
- Notes: Claude uses `--model` only on spawn; Codex/OpenCode read `session.model` per turn. Validate agent-specific constraints server-side.

---

## Current Behavior (Verified)

### Server session state is immutable for model/variant after creation
- Session state stores `model` and `variant` only when the session is created and never updates them afterward.
  - `server/packages/sandbox-agent/src/router.rs#L248` (`SessionState` fields)
  - `server/packages/sandbox-agent/src/router.rs#L294` (`SessionState::new` copies request.model / request.variant)
  - `server/packages/sandbox-agent/src/router.rs#L1480` (`create_session` builds `SessionState`)

### Model usage by agent
- **Codex**: model is sent per turn (`TurnStartParams.model`), so updating session state would apply immediately.
  - `server/packages/sandbox-agent/src/router.rs#L3005`
- **OpenCode**: model and variant are included per prompt in the HTTP request body, so updating session state would apply immediately.
  - `server/packages/sandbox-agent/src/router.rs#L3125`
- **Claude**: model is only applied at spawn via CLI (`--model`). Resumes use `--resume` and do not re-specify model beyond spawn options.
  - `server/packages/agent-management/src/agents.rs#L216`
- **Amp**: model is only applied at spawn (`--model`); resume uses `--continue`.
  - `server/packages/agent-management/src/agents.rs#L1000`

### Inspector UI only applies model/variant at creation
- The UI keeps editable inputs for `model` and `variant`, but only includes them in the `createSession` body.
- After session creation, edits only affect local state; no server update is sent.
  - `frontend/packages/inspector/src/App.tsx#L358`
  - `frontend/packages/inspector/src/components/chat/ChatSetup.tsx`

### No API endpoint exists to update sessions
- Router exposes POST create, send message, etc. but no PATCH or update endpoint.
  - `server/packages/sandbox-agent/src/router.rs#L90`
  - `docs/openapi.json#L186` (only `POST /v1/sessions/{session_id}`)

---

## Implementation Targets (Where to Change)

### 1) HTTP API: add session update endpoint
**Proposed**: `PATCH /v1/sessions/{session_id}`
- Add route in router:
  - `server/packages/sandbox-agent/src/router.rs#L90`
- Add to OpenAPI paths list:
  - `server/packages/sandbox-agent/src/router.rs#L140`
- Add request/response types (e.g., `UpdateSessionRequest`, `UpdateSessionResponse` or reuse `SessionInfo`):
  - `server/packages/sandbox-agent/src/router.rs#L3400`
- Add a `SessionManager::update_session(...)` mutation helper:
  - `server/packages/sandbox-agent/src/router.rs#L1780`

### 2) Update session state
- Modify `SessionState` fields for `model`/`variant` inside `update_session`.
- Return updated `SessionInfo` so UI can reflect the applied values.
  - `server/packages/sandbox-agent/src/router.rs#L1780`

### 3) Validation of agent constraints (server-side)
- **Claude/Amp**: model changes after the first upstream session id is created are likely ineffective.
  - Recommend: reject updates for Claude/Amp when `native_session_id` is present.
  - Alternative (riskier): clear `native_session_id` to force a new upstream session (breaks continuity).
- **OpenCode/Codex**: accept model updates at any time.
- **Variant**: only OpenCode consumes it; consider rejecting variant updates for other agents to avoid no-ops.
  - `server/packages/agent-management/src/agents.rs#L257`

### 4) Inspector UI: apply and reflect changes
- Add an action to call the new endpoint (button, `onBlur`, or debounced patch).
- Refresh or update local `sessions` state on success.
  - `frontend/packages/inspector/src/App.tsx#L280`
  - `frontend/packages/inspector/src/components/chat/ChatSetup.tsx`

### 5) SDK + CLI + Docs sync
- **TypeScript SDK**: add `updateSession` to `sdks/typescript/src/client.ts` and export types.
  - `sdks/typescript/src/client.ts#L90`
  - `sdks/typescript/src/types.ts`
  - `sdks/typescript/src/index.ts`
- **OpenAPI types**: regenerate `docs/openapi.json` and `sdks/typescript/src/generated/openapi.ts`.
- **CLI**: add `sandbox-agent api sessions update` and implement PATCH support in `ClientContext`.
  - `server/packages/sandbox-agent/src/main.rs#L144`
  - `server/packages/sandbox-agent/src/main.rs#L905`
- **Docs**:
  - Update `docs/cli.mdx` and `CLAUDE.md` CLI↔HTTP map.

### 6) Tests (required by repo rules)
- Add HTTP test coverage for the new PATCH route.
  - `server/packages/sandbox-agent/tests/sessions/`
- Add SDK test for `updateSession` request shape.
  - `sdks/typescript/tests/client.test.ts`

---

## Design Decisions to Confirm

1. Should Claude/Amp updates be rejected after the upstream session is created, or should we clear `native_session_id` and allow new session creation (at the cost of continuity)?
2. Should `variant` updates be rejected for non-OpenCode agents?
3. Should PATCH support explicit clearing of `model`/`variant` (e.g., `null`), or only set?

---

## Files Referenced

- `server/packages/sandbox-agent/src/router.rs`
- `server/packages/agent-management/src/agents.rs`
- `frontend/packages/inspector/src/App.tsx`
- `frontend/packages/inspector/src/components/chat/ChatSetup.tsx`
- `docs/openapi.json`
- `docs/cli.mdx`
- `CLAUDE.md`
- `sdks/typescript/src/client.ts`
- `sdks/typescript/src/types.ts`
- `sdks/typescript/src/index.ts`
- `sdks/typescript/tests/client.test.ts`
