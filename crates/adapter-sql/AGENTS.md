# SQL / Postgres adapter — agent binding guide

Profile:

```yaml
warehouse_pg:
  adapter: sql
  dsn: env:AG_SQL_DSN
```

## Introspect (MCP API)

```json
{ "source_id": "warehouse_pg", "schema_name": "public" }
```

`schema_name` is the Postgres **schema name** (default `public`). Optional for introspect — not a binding YAML field.

## Binding YAML rules

| Field | Meaning |
|-------|---------|
| `source_id` | Profile key, e.g. `warehouse_pg` |
| `entities.*.from` | SQL **table name** |
| `entities.*.id` | Primary key column (or logical id column) |
| `entities.*.fields` | Playbook field → column name |
| `relationships.*.link_column` | Foreign-key column on the object table |

**Do not use:** DSN in binding, raw SQL in `lookup`/`operations`, top-level `adapter`.

Binding file: `bindings/{playbook_id}.postgres.yaml` (suffix `postgres` for adapter `sql`).

Template: `get_binding("crm-payroll-access.postgres")`.
