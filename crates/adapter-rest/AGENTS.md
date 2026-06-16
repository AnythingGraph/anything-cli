# REST / HTTP JSON adapter — agent binding guide

Profile:

```yaml
api_main:
  adapter: rest
  base_url: env:AG_REST_BASE_URL
  auth: env:AG_REST_TOKEN
```

## Introspect

```json
{ "source_id": "api_main", "schema_name": "/users" }
```

`schema_name` optionally hints at a resource path prefix for catalog discovery.

## Binding YAML rules

| Field | Meaning |
|-------|---------|
| `source_id` | Profile key |
| `entities.*.from` | REST **resource path** (e.g. `/users`, `/accounts`) |
| `entities.*.fields` | Playbook field → JSON response field name |
| `relationships.*.link_column` | Query parameter or JSON field linking subject to object |

**Do not use:** base_url in binding, hand-written GET strings in authored YAML.

Binding file: `bindings/{playbook_id}.rest.yaml`

Operations compile to `GET ...` templates automatically.
