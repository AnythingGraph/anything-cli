# MongoDB adapter — agent binding guide

Profile:

```yaml
mongo_main:
  adapter: mongodb
  dsn: env:AG_MONGODB_DSN
  database: env:AG_MONGODB_DATABASE
```

## Introspect

```json
{ "source_id": "mongo_main", "schema_name": "my_database" }
```

`schema_name` is the MongoDB **database name** (or use profile `database` field).

## Binding YAML rules

| Field | Meaning |
|-------|---------|
| `source_id` | Profile key |
| `entities.*.from` | **Collection name** |
| `entities.*.fields` | Playbook field → document field path |

Binding file: `bindings/{playbook_id}.mongodb.yaml`

Operations compile to `find:` / `count:` templates — do not author them manually.
