# Playbooks for ag-cli

## Included playbooks

| Playbook | Sources | Bindings |
|----------|---------|----------|
| `simple-crm-access` | Postgres | `simple-crm-access.postgres.yaml` |
| `crm-payroll-access` | Postgres + CSV | `crm-payroll-access.postgres.yaml`, `crm-payroll-access.csv.yaml` |

### CRM + payroll (two bindings, one playbook)

- **Postgres:** `crm_user`, `crm_account`, relationship `owns_account`
- **CSV:** `crm_payroll_record`, relationship `user_has_payroll` (file: `data/payroll.csv`)

Example MCP queries (binding_name is optional — auto-routed from entity_sources + bindings):

```text
# Accounts in Postgres (routes via crm_account → postgres binding)
query_graph playbook_id=crm-payroll-access
  entity=crm_user by_name="Alex Anderson"
  count_relationship=owns_account count_object_entity=crm_account

# Payroll in CSV (routes via crm_payroll_record → csv binding)
query_graph playbook_id=crm-payroll-access
  entity=crm_user by_name="Alex Anderson"
  count_relationship=user_has_payroll count_object_entity=crm_payroll_record
```

Environment:

```bash
export AG_SQL_DSN="postgres://..."
export AG_PAYROLL_CSV_PATH="$(pwd)/data/payroll.csv"
```

## Quick test

From `ag-cli/`:

```bash
cargo run -p anythinggraph-ag -- validate --playbooks playbooks
```
