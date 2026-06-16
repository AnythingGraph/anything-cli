# CSV adapter — agent binding guide

Profile:

```yaml
payroll_csv:
  adapter: csv
  file_path: env:AG_PAYROLL_CSV_PATH
```

## Introspect

```json
{ "source_id": "payroll_csv" }
```

No `schema_name` required — introspect reads column headers from the file.

## Binding YAML rules

| Field | Meaning |
|-------|---------|
| `source_id` | Profile key, e.g. `payroll_csv` |
| `entities.*.from` | CSV **filename** (e.g. `payroll.csv`) |
| `entities.*.fields` | Map playbook field → CSV column when names differ |

**Do not use:** file path in binding (path is in profile only), raw SQL.

Binding file: `bindings/{playbook_id}.csv.yaml`

For federated playbooks, the CSV binding often includes both `crm_user` and `payroll_record` from the same file so one binding can resolve the subject and count payroll.

Template: `get_binding("crm-payroll-access.csv")` or `sales-compensation-access.csv`.
