# AGENTS.md — Anything CLI MCP authoring guide

Instructions for AI agents using **anythinggraph-thin** MCP to create playbooks and bindings.

## Golden rules

1. **Use compact declarative format only** — never author legacy verbose bindings with raw SQL.
2. **`propose_*` validates; `save_*` persists** — save the **same YAML/JSON you wrote**, not expanded output from propose responses.
3. **Never save `debug_compiled_binding_yaml`** — it is omitted by default; when present it is debug-only.
4. **Profiles are manual** — edit `profiles/local.yaml` yourself; use `list_sources` + `introspect_source` to discover schema.
5. **Copy demos first** — call `get_binding("crm-payroll-access.postgres")` and `get_binding("crm-payroll-access.csv")` before authoring new bindings.

## Admin workflow

```
list_sources → introspect_source(source_id)
→ propose_playbook → save_playbook
→ get_playbook_context → suggest_bindings
→ propose_binding → test_binding → save_binding
```

Reference examples: playbook `crm-payroll-access`, bindings `crm-payroll-access.postgres` / `.csv`.

## Playbook JSON (compact)

```json
{
  "id": "my-playbook",
  "name": "My playbook",
  "description": "What agents can query.",
  "entities": {
    "crm_user": { "id": "user_id", "fields": ["full_name"] },
    "crm_account": { "id": "account_name", "fields": ["industry"] }
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

**Do not use for new playbooks:** `entities[]` arrays, `entity_relationships[]`, `entity_sources` + separate `bindings` map (legacy — still loads, but avoid).

**File:** `playbooks/{id}.json`

## Binding YAML (compact)

One file per source key: `bindings/{playbook_id}.{source_key}.yaml`

### Postgres / SQL

```yaml
source_id: warehouse_pg

entities:
  crm_user:
    from: users
    id: user_id
    fields: [full_name]

  crm_account:
    from: accounts
    id: account_name
    fields: [industry]

relationships:
  owns_account:
    object: crm_account
    link_column: owner_user_id
```

### CSV

Map fields only when the CSV column name differs from the playbook field:

```yaml
source_id: payroll_csv

entities:
  payroll_record:
    from: payroll.csv
    id: payroll_id
    fields:
      user_id: user

relationships:
  user_has_payroll:
    object: payroll_record
    link_column: user
```

## Do NOT include in new bindings

| Legacy / verbose | Why |
|------------------|-----|
| `adapter`, `version`, `playbook_id` at top | Inferred from profile + filename |
| `id_field` | Use `id` |
| `lookup:` / `operations:` with SQL | Auto-compiled from declarative fields |
| `relationships.*.join` + explicit SQL | Use `object` + `link_column` only |
| `SELECT ...` strings in CSV bindings | CSV adapter uses declarative `from` + `fields` |

## MCP tools

| Tool | Save what |
|------|-----------|
| `propose_playbook` | Validates only — read `save_instruction` |
| `save_playbook` | Your compact JSON (same as proposed input) |
| `propose_binding` | Validates only — read `save_instruction` |
| `save_binding` | Your compact YAML + `adapter_suffix` (source key) — **saved verbatim**, not compiled/expanded |

## Validate after save

```bash
cargo run -p anythinggraph-ag -- validate --playbooks playbooks
```

Or reload reasoning-service catalog (restart `./start-all.sh`).

## Auth

Admin token required for authoring tools. User token is query-only (`query_graph`, `list_allowed_rows`).

See [README.md](README.md) for `AG_ADMIN_TOKENS` / `AG_USER_TOKENS` setup.
