# ag-cli — AnythingGraph thin reasoning layer

Self-contained **Reasoning as Code** stack: Rust core (playbook → plan → SQL/SOQL adapters) + TypeScript MCP front-end.

Does **not** modify the main OSS monorepo services. Lives entirely under `ag-cli/`.

## Architecture

```text
AI agent (Cursor, Claude)
        │ MCP (TypeScript, mcp/)
        ▼
reasoning-service (Rust HTTP, :8787)
        │ plan IR + adapters
        ▼
Source systems (Postgres, Salesforce, …) via bindings/*.yaml
```

### Rust crates

| Crate | Role |
|-------|------|
| `playbook-spec` | Load / validate playbook JSON |
| `binding-spec` | Load binding YAML + profile |
| `plan-ir` | Source-agnostic query plan |
| `plan-compiler` | Query request → plan |
| `proof` | Answer + evidence envelope |
| `adapter-core` | Adapter trait + registry |
| `adapter-sql` | Postgres / SQL bindings |
| `adapter-soql` | Salesforce SOQL bindings |
| `adapter-csv` | CSV file bindings |
| `runtime` | Orchestrator |
| `reasoning-service` | HTTP API |
| `anythinggraph-ag` (`ag`) | CLI |

### MCP tools (TypeScript)

| Tool | Description |
|------|-------------|
| `health_check` | Ping Rust service |
| `list_sources` | Profile sources (Postgres, Salesforce, …) |
| `list_bindings` / `get_binding` | Loaded binding files |
| `get_playbook_context` | Playbook entities + relationships |
| `introspect_source` | Postgres tables, columns, foreign keys |
| `suggest_bindings` | Entity → table mapping suggestions |
| `propose_binding` | Validate + compile binding YAML (no save) |
| `test_binding` | Dry-run or live-test a binding |
| `save_binding` | Write `bindings/{playbook_id}.{adapter}.yaml` |
| `plan_query` | Compile plan IR |
| `execute_plan` | Run plan via adapters |
| `query_graph` | Plan + execute in one step |

## Quick start

### Start both services (recommended)

```bash
cd ag-cli
export AG_SQL_DSN="postgres://user:pass@localhost:5432/yourdb"
chmod +x start-all.sh   # first time only
./start-all.sh
```

This starts **reasoning-service** (`:8787`) and **MCP HTTP** (`:3334/mcp`). Ctrl+C stops both.

Defaults (override via env):

| Variable | Default |
|----------|---------|
| `AG_PAYROLL_CSV_PATH` | `./data/payroll.csv` |
| `AG_REASONING_URL` | `http://127.0.0.1:8787` |
| `AG_MCP_PORT` | `3334` |

### Manual start (alternative)

#### 1. Playbooks

Local starter playbooks: `simple-crm-access` (Postgres only) and `crm-payroll-access` (Postgres + CSV).

#### 2. Build Rust

```bash
cd ag-cli
cargo build --release
```

#### 3. Start reasoning-service

```bash
export AG_PLAYBOOKS_DIR="../dashboard/backend/src/playbook/playbooks"
export AG_SQL_DSN="postgres://user:pass@localhost:5432/crm"
cargo run -p reasoning-service --release
```

#### 4. Start MCP (HTTP)

```bash
cd mcp
npm install
export AG_REASONING_URL=http://127.0.0.1:8787
npm run dev:http
```

MCP endpoint: `http://127.0.0.1:3334/mcp`

#### 5. CLI helpers

```bash
cargo run -p anythinggraph-ag -- validate --playbooks ../dashboard/backend/src/playbook/playbooks

cargo run -p anythinggraph-ag -- test \
  --playbooks ../dashboard/backend/src/playbook/playbooks \
  --playbook-id crm-relationship-access \
  --by-name "Alex Anderson" \
  --relationship owns_account

cargo run -p anythinggraph-ag -- mcp-config
```

## Agent binding onboarding (MCP workflow)

External agents (Claude, Cursor, OpenAI) can map a playbook to a customer data source:

1. `get_playbook_context(playbook_id)` — semantic entities/relationships
2. `list_sources` → `introspect_source(source_id)` — physical schema catalog
3. `suggest_bindings(playbook_id, source_id)` — heuristic entity→table hints
4. Agent drafts YAML → `propose_binding` → `test_binding(execute=true)`
5. `save_binding(playbook_id, adapter_suffix, binding_yaml)` — persists `bindings/{playbook_id}.postgres.yaml`

**Declarative bindings:** set `from`, `id_field`, `fields`, and `subject_link_column`; Rust compiles lookup/count/list SQL automatically. Full SQL strings still supported.

Playbooks can declare a **bindings** map (source key → binding file stem) plus **entity_sources**. When `binding_name` is omitted, the runtime routes from the count/list object entity → source → binding.

Example in `crm-payroll-access.json`:

```json
"entity_sources": { "crm_account": "postgres", "crm_payroll_record": "csv" },
"bindings": {
  "postgres": "crm-payroll-access.postgres",
  "csv": "crm-payroll-access.csv"
}
```

`default_binding` remains a fallback when auto-routing cannot infer a binding.

## HTTP API (reasoning-service)

| Method | Path | Body |
|--------|------|------|
| GET | `/health` | — |
| GET | `/sources` | — |
| POST | `/sources/{id}/introspect` | `{ "schema_name": "public" }` |
| GET | `/bindings` | — |
| GET | `/bindings/{name}` | — |
| GET | `/playbooks/{id}/context` | — |
| POST | `/playbooks/{id}/suggest-bindings` | `{ "source_id", "schema_name?" }` |
| POST | `/playbooks/{id}/propose-binding` | `{ "binding_yaml" }` |
| POST | `/playbooks/{id}/test-binding` | `{ "binding_yaml?", "binding_name?", "execute?" }` |
| POST | `/playbooks/{id}/save-binding` | `{ "adapter_suffix", "binding_yaml" }` |
| POST | `/plan` | `QueryRequest` JSON |
| POST | `/execute` | `{ "plan": ... }` |
| POST | `/query` | `QueryRequest` JSON |

## Environment

| Variable | Default | Purpose |
|----------|---------|---------|
| `AG_PLAYBOOKS_DIR` | `ag-cli/playbooks` | Playbook JSON catalog |
| `AG_BINDINGS_DIR` | `ag-cli/bindings` | Binding YAML files |
| `AG_PROFILE_PATH` | `ag-cli/profiles/local.yaml` | Source credentials |
| `AG_SQL_DSN` | — | Postgres for `adapter: sql` |
| `AG_SF_INSTANCE_URL` | — | Salesforce instance |
| `AG_SF_ACCESS_TOKEN` | — | Salesforce token |
| `AG_PAYROLL_CSV_PATH` | — | CSV file for `adapter: csv` (e.g. `data/payroll.csv`) |
| `AG_REASONING_URL` | `http://127.0.0.1:8787` | MCP → Rust |
| `AG_MCP_PORT` | `3334` | Thin MCP HTTP port |

## Adding a new data source

1. Add adapter crate (`adapter-mongo`, etc.) implementing `DataAdapter`.
2. Register in `runtime/src/lib.rs`.
3. Add profile entry in `profiles/local.yaml`.
4. Use MCP onboarding tools to generate `bindings/{playbook_id}.{adapter}.yaml`, or author YAML manually.

## License

Apache-2.0
