# Required Tests

- `test_agents_install_version_spawn` (installs, checks version, spawns prompt for Claude/Codex/OpenCode; Amp spawn runs only if `~/.amp/config.json` exists)
- daemon http api: smoke tests for each endpoint response shape/status
- cli: subcommands hit expected endpoints and handle error responses
