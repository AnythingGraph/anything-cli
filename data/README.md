# Sample data for ag-cli

| File | Used by |
|------|---------|
| `payroll.csv` | Playbook `crm-payroll-access` via binding `crm-payroll-access.csv` |

Set the path before starting reasoning-service:

```bash
export AG_PAYROLL_CSV_PATH="/absolute/path/to/ag-cli/data/payroll.csv"
```

Postgres tables (`users`, `accounts`) use column `user_id`. The payroll CSV uses column **`user`** — the CSV binding maps playbook field `user_id` → CSV column `user` in `fields`.
