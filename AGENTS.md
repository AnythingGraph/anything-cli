# AGENTS.md — Anything CLI MCP authoring guide

Instructions for AI agents using **anythinggraph-thin** MCP to create playbooks and bindings.

## Golden rules

1. **Use MCP tools only** — when connected to **anythinggraph-thin** MCP, call tools (`introspect_source`, `query_graph`, etc.). Do **not** use `curl`, `fetch`, or Python scripts against `http://127.0.0.1:8787` unless the user explicitly asks for a standalone script or MCP is unavailable.
2. **Use compact declarative format only** — never author legacy verbose bindings with raw SQL.
3. **`propose_*` validates; `save_*` persists** — save the **same YAML/JSON you wrote**, not expanded output from propose responses.
4. **Never save `debug_compiled_binding_yaml`** — it is omitted by default; when present it is debug-only.
5. **Profiles are manual** — edit `profiles/local.yaml` yourself; put secrets in `.env` (copy from `.env.example`, gitignored). Use `list_sources` + `get_adapter_guide` + `introspect_source` + `sample_source` to discover schema and example data.
6. **Use the right template for each adapter** — SQL/CSV: `get_binding("crm-payroll-access.postgres")` and `.csv`. Mongo/REST/SOQL: **`get_adapter_guide(source_id)`** (`example_binding_yaml` + `instructions_markdown`).
7. **Per-adapter rules live in adapter crates** — call **`get_adapter_guide(source_id)`** after `list_sources`; do not guess binding shape from introspect API params alone.
8. **`test_binding(execute=true)` before save** — use real identifier values from the live source when possible.

## Exploring a source (no playbook)

When the user asks what data lives in a source (e.g. “what’s in `mongo_main`?”):

```
list_sources
→ introspect_source(source_id, schema_name?)   # collections / tables / columns
→ sample_source(source_id, resource=<table|collection|object>, limit=5)   # raw example rows
```

Do **not** call `list_playbooks` for source exploration. Playbooks are only needed once you map entities and run `query_graph`.

## Admin workflow

```
list_sources
→ get_adapter_guide(source_id)   # once per source you will bind — REQUIRED before propose_binding
→ introspect_source(source_id, schema_name?)   # schema_name meaning comes from get_adapter_guide
→ propose_playbook → save_playbook
→ get_playbook_context → suggest_bindings
→ propose_binding → test_binding(execute=true) → save_binding
```

**Discovering `source_id`:** always from `list_sources` (profile keys like `warehouse_pg`, `payroll_csv`). Never invent source ids.

**Adapter-specific binding rules:** returned by `get_adapter_guide` as `instructions_markdown` + `example_binding_yaml`. Source markdown also lives under `crates/adapter-*/AGENTS.md` in the repo.

Reference examples: playbook `crm-payroll-access`, bindings `crm-payroll-access.postgres` / `.csv`.

## Playbook JSON (compact)

Playbook entities use **`identifier`** (logical id field name) and **`attributes`** (other readable fields). Bindings keep **`id`** / **`fields`** for physical column/property mapping.

```json
{
  "id": "my-playbook",
  "name": "My playbook",
  "description": "What agents can query.",
  "entities": {
    "crm_user": { "identifier": "user_id", "attributes": ["full_name"] },
    "crm_account": { "identifier": "account_name", "attributes": ["industry"] }
  },
  "relationships": {
    "owns_account": { "from": "crm_user", "to": "crm_account" }
  },
  "sources": {
    "crm_user": "postgres",
    "crm_account": "postgres"
  },
  "access": {
    "summary": "Users read accounts they own.",
    "subject": "crm_user",
    "subject_id": "user_id",
    "allow": [
      { "relationship": "owns_account", "resource": "crm_account" }
    ]
  }
}
```

**File:** `playbooks/{id}.json`

## Binding YAML (compact)

One file per playbook source key: `bindings/{playbook_id}.{source_key}.yaml`

Allowed top-level keys: **`source_id`**, **`entities`**, **`relationships`** only (unless adapter guide says otherwise).

**Do not author:** `lookup`, `operations`, `adapter`, `playbook_id`, top-level `schema_name`, raw SQL/SOQL/HTTP strings.

See **`get_adapter_guide(source_id)`** for what `entities.*.from` means per adapter (SQL table, CSV filename, MongoDB collection, etc.).

## Adapter pitfalls (compact bindings)

| Adapter | Before `save_binding` |
|---------|------------------------|
| **SQL / CSV** | `introspect_source`; map `from` to table/file; demo bindings are good templates |
| **Mongo / REST / SOQL** | `get_adapter_guide` workflow; do not copy SQL binding shape |

## MCP tools (not HTTP)

| Tool | When |
|------|------|
| `list_sources` | Discover profile `source_id` + adapter type; read `authoring_next_step` |
| **`get_adapter_guide`** | **After list_sources, before propose_binding** — per-source binding rules |
| `introspect_source` | Live schema; `schema_name` meaning is adapter-specific (see guide) |
| `sample_source` | Raw row preview from one table/collection/object — **no playbook** |
| `propose_playbook` / `save_playbook` | Playbook JSON |
| `suggest_bindings` | Heuristic entity mapping |
| `propose_binding` / `test_binding` / `save_binding` | Binding YAML |
| `list_entity` | Browse rows for one entity (default limit 1000) — no lookup sweep |
| `sample_entity` | Small row sample for discovery (default limit 5) |
| `query_graph` | Resolve entity + optional relationship count/list |

## Validate after save

```bash
cargo run -p anythinggraph-ag -- validate --playbooks playbooks
```

Or restart `./start-all.sh` after manual file edits.

## Auth

Admin token required for authoring tools. User token is query-only (`query_graph`, `list_entity`, `sample_entity`, `list_allowed_rows`).

See [README.md](README.md) for `AG_ADMIN_TOKENS` / `AG_USER_TOKENS` setup.
