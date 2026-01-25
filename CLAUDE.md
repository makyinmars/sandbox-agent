# Claude Code Instructions

## Agent Schemas

Agent schemas (Claude Code, Codex, OpenCode, Amp) are available for reference in `resources/agent-schemas/dist/`.

Research on how different agents operate (CLI flags, streaming formats, HITL patterns, etc.) is in `research/agents/`. When adding or making changes to agent docs, follow the same structure as existing files.

Universal schema guidance:
- The universal schema should cover the full feature set of all agents.
- Conversions must be best-effort overlap without being lossy; preserve raw payloads when needed.

## Spec Tracking

- Track required tests in `spec/required-tests.md` as you write code.
- Capture unresolved questions/ambiguities in `spec/im-not-sure.md`.
- Update `todo.md` as work progresses; add new tasks as they arise.
- Keep CLI subcommands in sync with every HTTP endpoint.

## Git Commits

- Do not include any co-authors in commit messages (no `Co-Authored-By` lines)
- Use conventional commits style (e.g., `feat:`, `fix:`, `docs:`, `chore:`, `refactor:`)
- Keep commit messages to a single line
