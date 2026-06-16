# SOQL / Salesforce adapter — agent binding guide

Profile:

```yaml
salesforce_main:
  adapter: soql
  instance_url: env:AG_SF_INSTANCE_URL
  auth: env:AG_SF_ACCESS_TOKEN
```

## Introspect

```json
{ "source_id": "salesforce_main", "schema_name": "Account" }
```

`schema_name` optionally filters to a Salesforce **object API name** (e.g. `Account`, `Contact`).

## Binding YAML rules

| Field | Meaning |
|-------|---------|
| `source_id` | Profile key |
| `entities.*.from` | Salesforce **object API name** |
| `entities.*.fields` | Playbook field → Salesforce field API name |
| `relationships.*.link_column` | Lookup/reference field on object |

**Do not use:** instance_url or token in binding, raw SOQL in authored YAML.

Binding file: `bindings/{playbook_id}.salesforce.yaml` (suffix `salesforce` for adapter `soql`).
