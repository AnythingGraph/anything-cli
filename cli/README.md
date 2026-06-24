# AnythingGraph CLI

OpenClaw-style installer and gateway for **Anything CLI (ag-cli)** — the thin reasoning layer between AI agents and your live data sources.

## Quick start

```bash
npm install -g @anythinggraph/cli@latest
anythinggraph onboard --install-daemon
```

Or use the bootstrap script:

```bash
curl -fsSL https://raw.githubusercontent.com/AnythingGraph/anything-cli/main/cli/install.sh | bash
```

## Commands

| Command | Description |
|---------|-------------|
| `anythinggraph onboard` | Clone/update anything-cli from GitHub, build reasoning-service |
| `anythinggraph onboard --install-daemon` | Onboard + macOS LaunchAgent / Linux systemd user service |
| `anythinggraph start` | Start reasoning-service + MCP HTTP in the foreground (Ctrl+C to stop) |
| `anythinggraph stop` | Stop supervised services |
| `anythinggraph status` | Show health URLs |
| `anythinggraph doctor` | Prerequisites + service health checks |
| `anythinggraph source add` | Interactive wizard: pick adapter, name source, enter credentials, validate, save |
| `anythinggraph source remove` | List sources with status, pick by number, remove from profile and `.env` |
| `anythinggraph sources` | List configured sources from `profiles/local.yaml` and validate each connection |
| `anythinggraph mcp print-config` | Cursor MCP JSON |
| `anythinggraph mcp print-config --target claude` | Claude Desktop `mcp-remote` bridge JSON |

## What gets installed

On `onboard`, the CLI **always** uses Git — no local path overrides:

1. **Clone** https://github.com/AnythingGraph/anything-cli into `~/.anythinggraph/source` (first run)
2. **`git pull --ff-only`** on that directory when you run onboard again

It writes `~/.anythinggraph/config.json` pointing at that checkout.

`start` launches the same stack as `./start-all.sh`:

- **Rust:** `reasoning-service` (built into `~/.anythinggraph/bin/`)
- **Node:** ag-cli MCP HTTP (`mcp/` → port 3334)

It does **not** install or start the legacy AnythingGraph dashboard, `mcp-service`, or `core-services`.

### Data sources

With the stack running (`anythinggraph start`), add a source interactively:

```bash
anythinggraph source add
```

The wizard walks through four steps: choose adapter type, pick a `source_id`, enter connection details, then test the connection before writing `profiles/local.yaml` and `.env`.

You can still edit those files by hand — see [connect-data.html](https://www.anythinggraph.com/connect-data.html) on the website.

`anythinggraph start` loads `.env` from the git checkout automatically.

Use `anythinggraph start --rebuild-rust` after pulling Rust changes.

## Environment

| Variable | Purpose |
|----------|---------|
| `ANYTHINGGRAPH_HOME` | Override `~/.anythinggraph` (checkout still lives under `{home}/source`) |

## Default ports

| Service | URL |
|---------|-----|
| Reasoning API | `http://127.0.0.1:8787` |
| MCP (Cursor / Claude) | `http://127.0.0.1:3334/mcp` |

## Notes

- **Rust (`cargo`)** and **git** are required on first onboard.
- **Daemon** is supported on macOS (launchd) and Linux (systemd user). On Windows, use WSL2 or run `anythinggraph start` in a terminal.
- Connect MCP in Cursor or Claude, then ask agents to use playbook tools (`query_graph`, `introspect_source`, etc.). See **AGENTS.md** in the anything-cli repo.
