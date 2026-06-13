# Playbooks

Sample playbooks for AnythingGraph CLI. Setup and MCP usage: **[main README](../README.md)**.

| Playbook | Sources |
|----------|---------|
| `simple-crm-access` | Postgres |
| `crm-payroll-access` | Postgres + CSV |

---

## How to write a playbook and bindings

A **playbook** (`playbooks/<id>.json`) is your business vocabulary and routing rules. **Bindings** (`../bindings/<playbook_id>.<source>.yaml`) map that vocabulary to real tables or files. Credentials live in `../profiles/local.yaml`.

Working example in this folder: `crm-payroll-access.json` with `../bindings/crm-payroll-access.postgres.yaml` and `../bindings/crm-payroll-access.csv.yaml`.

### 1. Playbook JSON — what you model

| Block | Purpose |
|-------|---------|
| `id`, `name`, `description` | Playbook identity |
| `entities[]` | Things in your domain (`crm_user`, `crm_account`, …) and their fields |
| `entity_relationships[]` | How entities connect (`owns_account`: user → account) |
| `entity_sources` | Which source key each entity lives on (`postgres`, `csv`, …) |
| `bindings` | Maps source keys → binding file stems (no `.yaml`) |
| `default_binding` | Fallback when routing cannot infer a binding |
| `relationship_access_rules` | Optional ReBAC (who may read which rows) |

Minimal shape (CRM + payroll across Postgres and CSV):

```json
{
  "id": "crm-payroll-access",
  "name": "CRM + payroll access",
  "description": "Users, accounts in Postgres; payroll in CSV.",
  "entities": [
    {
      "name": "crm_user",
      "display_name": "CRM user",
      "fields": [
        { "field_name": "user_id", "field_type": "TEXT", "is_identifier": true },
        { "field_name": "full_name", "field_type": "TEXT" }
      ]
    },
    {
      "name": "crm_account",
      "display_name": "Account",
      "fields": [
        { "field_name": "account_name", "field_type": "TEXT", "is_identifier": true },
        { "field_name": "industry", "field_type": "TEXT" }
      ]
    },
    {
      "name": "crm_payroll_record",
      "display_name": "Payroll record",
      "fields": [
        { "field_name": "payroll_id", "field_type": "TEXT", "is_identifier": true },
        { "field_name": "user_id", "field_type": "TEXT" }
      ]
    }
  ],
  "entity_relationships": [
    {
      "relationship_name": "owns_account",
      "subject_entity_name": "crm_user",
      "object_entity_name": "crm_account"
    },
    {
      "relationship_name": "user_has_payroll",
      "subject_entity_name": "crm_user",
      "object_entity_name": "crm_payroll_record"
    }
  ],
  "entity_sources": {
    "crm_user": "postgres",
    "crm_account": "postgres",
    "crm_payroll_record": "csv"
  },
  "bindings": {
    "postgres": "crm-payroll-access.postgres",
    "csv": "crm-payroll-access.csv"
  },
  "default_binding": "crm-payroll-access.postgres"
}
```

**Field names** in the playbook are the stable vocabulary agents use. Physical column names are mapped in binding YAML (`fields` below).

**Optional ReBAC** — add `relationship_access_rules` with `implementation_status: "enforced"` and allow rules that walk `entity_relationships` paths. See `crm-payroll-access.json` for a full example.

### 2. Binding YAML — where data lives

Each binding file:

- Names the **adapter** (`sql`, `csv`, `soql`, …)
- Points at a **profile source** (`source_id` → `profiles/local.yaml`)
- Maps each **playbook entity** to a table or file
- Declares **relationships** with a link column for per-user counts/lists

**Postgres** (`../bindings/crm-payroll-access.postgres.yaml`):

```yaml
adapter: sql
playbook_id: crm-payroll-access
source_id: warehouse_pg

entities:
  crm_user:
    from: users
    id_field: user_id
    fields:
      user_id: user_id
      full_name: full_name

  crm_account:
    from: accounts
    id_field: account_name
    fields:
      account_name: account_name
      industry: industry

relationships:
  owns_account:
    join:
      from_entity: crm_user
      to_entity: crm_account
      on: "accounts.owner_user_id = users.user_id"
    subject_link_column: owner_user_id
```

- `from` — physical table name
- `fields` — playbook field → column name (`user_id: user_id`)
- `subject_link_column` — on the **object** table/file, column that points at the subject’s id (used to compile count/list queries)

**CSV** (`../bindings/crm-payroll-access.csv.yaml`):

```yaml
adapter: csv
playbook_id: crm-payroll-access
source_id: payroll_csv

entities:
  crm_payroll_record:
    from: payroll.csv
    id_field: payroll_id
    fields:
      payroll_id: payroll_id
      user_id: user          # playbook user_id → CSV column "user"
      pay_period: pay_period
      gross_pay: gross_pay

relationships:
  user_has_payroll:
    join:
      from_entity: crm_user
      to_entity: crm_payroll_record
      on: "payroll.user = user.user"
    subject_link_column: user
```

When the CSV column name differs from the playbook (`user` vs `user_id`), map it in `fields` — left side is playbook field, right side is physical column.

You do **not** need to write SQL for lookups or counts: with `from`, `id_field`, `fields`, and `subject_link_column`, Rust compiles the queries automatically.

### 3. Profile — credentials

`../profiles/local.yaml` registers sources referenced by `source_id`:

```yaml
sources:
  warehouse_pg:
    adapter: sql
    dsn: env:AG_SQL_DSN
  payroll_csv:
    adapter: csv
    file_path: env:AG_PAYROLL_CSV_PATH
```

Set env vars before starting (`AG_SQL_DSN`, `AG_PAYROLL_CSV_PATH`, etc.).

### 4. File layout

```text
playbooks/crm-payroll-access.json
bindings/crm-payroll-access.postgres.yaml
bindings/crm-payroll-access.csv.yaml
profiles/local.yaml
```

Binding file stem must match the playbook’s `bindings` map (e.g. key `postgres` → stem `crm-payroll-access.postgres`).

### 5. Validate and test

From the ag-cli root:

```bash
cargo run -p anythinggraph-ag -- validate --playbooks playbooks
./start-all.sh
```

Then via MCP `query_graph` (resolve user by name, count a relationship) or:

```bash
curl -s http://127.0.0.1:8787/query \
  -H 'Content-Type: application/json' \
  -d '{
    "playbook_id": "crm-payroll-access",
    "resolve": { "entity": "crm_user", "by_name": "Alex Anderson" },
    "count": { "relationship": "owns_account", "object_entity": "crm_account" }
  }'
```

Omit `binding_name` on queries — the runtime picks the binding from `entity_sources` + `bindings` based on the object entity.

For agent-assisted mapping, use MCP: `get_playbook_context` → `introspect_source` → `suggest_bindings` → `propose_binding` → `test_binding` → `save_binding`.
