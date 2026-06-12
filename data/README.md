# Sample data for ag-cli

| File | Used by |
|------|---------|
| `payroll.csv` | Playbook `crm-payroll-access` via binding `crm-payroll-access.csv` |

Set the path before starting reasoning-service:

```bash
export AG_PAYROLL_CSV_PATH="/absolute/path/to/ag-cli/data/payroll.csv"
```

Postgres tables (`users`, `accounts`) use column `user_id`. The payroll CSV uses column **`user`** — the playbook `field_mappings.payroll_csv` maps playbook field `user_id` → CSV column `user`. The CSV binding implements that mapping.
