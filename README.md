# AnythingGraph CLI

**A thin semantic layer between your AI agent and your data.**

Your data stays where it is — Postgres, Salesforce, CSV files, and more. AnythingGraph gives Cursor, Claude, and other agents a **governed way to understand and query** that data, with answers you can trace back to the source.

No data lake. No ETL project. No copy-paste SQL into chat.

---

## What you get

| Today | With AnythingGraph |
|-------|-------------------|
| Agent guesses table names and writes SQL | Agent reads a **playbook** — your business vocabulary |
| Data scattered across systems | One question can span **multiple sources** (e.g. CRM in Postgres + payroll in CSV) |
| “Trust me” answers | **Evidence** — which source was queried and what came back |
| Every customer re-explains their schema | **Bindings** map your playbook once; reuse forever |

---

## Three ideas (high level)

### 1. Playbook — the use case

A playbook describes **what you care about** in business terms:

- *Who is the customer?*
- *What accounts do they own?*
- *What payroll records exist for them?*

Included examples in `playbooks/`:

| Playbook | Story |
|----------|--------|
| `simple-crm-access` | Sales rep → accounts they own |
| `crm-payroll-access` | Same rep → accounts **and** payroll history from a CSV file |

Playbooks are **portable**. The same playbook can point at different customer databases via bindings.

### 2. Ontology — the vocabulary

Inside each playbook, an **ontology** is the shared language:

- **Entities** — things in your world (`crm_user`, `crm_account`, `crm_payroll_record`)
- **Relationships** — how they connect (`owns_account`, `user_has_payroll`)

Agents don’t need to know your table layout. They ask in playbook terms; AnythingGraph translates.

### 3. Data bindings — where data actually lives

A **binding** connects playbook concepts to **your** systems:

- Postgres `users` / `accounts` tables
- A `payroll.csv` file where the user column is named `user` instead of `user_id`

**One playbook, multiple bindings** — e.g. CRM in Postgres, payroll in CSV. The agent picks the right source automatically based on what you’re asking about.

```
Playbook (what)     →  Ontology (vocabulary)  →  Bindings (where)
crm-payroll-access     user, account, payroll      postgres + csv
```

---

## Try it in 2 minutes

**1. Start the stack**

```bash
export AG_SQL_DSN="postgres://user:pass@localhost:5432/yourdb"
chmod +x start-all.sh   # first time only
./start-all.sh
```

**2. Connect MCP in Cursor** — add server URL: `http://127.0.0.1:3334/mcp`

Or print a config snippet:

```bash
cargo run -p anythinggraph-ag -- mcp-config
```

**3. Ask your agent** (copy-paste):

> Use anythinggraph-thin MCP. For playbook **crm-payroll-access**, tell me how many accounts and how many payroll records **Alex Anderson** has.

The agent calls **`query_graph`** twice — Postgres for accounts, CSV for payroll — and returns counts with proof.

---

## MCP tools — what to ask your agent

Connect **anythinggraph-thin** MCP, then use natural language. These are the high-impact flows:

### Ask questions (most common)

| You say | MCP tool | What happens |
|---------|----------|--------------|
| “How many accounts does Alex own?” | `query_graph` | Queries Postgres via playbook |
| “Show Alex’s payroll history count” | `query_graph` | Queries CSV via playbook |
| “Is the service up?” | `health_check` | Pings reasoning layer |

**Example prompts**

```
For playbook crm-payroll-access: how many accounts does Alex Anderson own?
```

```
For playbook crm-payroll-access: how many payroll records does Alex Anderson have?
```

```
For playbook simple-crm-access: resolve user Alex Anderson and count owned accounts.
```

### Connect your own data (agent-assisted setup)

| You say | MCP tools used |
|---------|----------------|
| “What data sources are configured?” | `list_sources` |
| “What tables are in my Postgres?” | `introspect_source` |
| “Map crm-payroll-access to my database” | `get_playbook_context` → `suggest_bindings` → `propose_binding` → `test_binding` → `save_binding` |

**Example prompt**

```
Using anythinggraph-thin: load playbook crm-payroll-access, inspect my Postgres
source, suggest how to map entities to my tables, test the binding, and save it.
```

### Explore before querying

| You say | MCP tool |
|---------|----------|
| “What entities are in this playbook?” | `get_playbook_context` |
| “What bindings exist?” | `list_bindings` |

---

## Why agents love this

1. **Stable vocabulary** — “customer” and “owns_account” don’t change when IT renames a column (update the binding, not every prompt).
2. **Federated** — one playbook, many systems; no forcing everything into one database.
3. **Onboarding loop** — agent can read your schema, propose a binding, test it, and save — you stay in the loop, the agent does the tedious mapping.
4. **Reasoning layer** — queries go through a governed path (playbook → plan → source), not raw SQL from chat.

---

## Technical reference

### Install & run

**Requirements:** Rust, Node.js, Postgres (for CRM demos), optional CSV for payroll demo.

```bash
cargo build --release
cd mcp && npm install && cd ..

export AG_SQL_DSN="postgres://user:pass@localhost:5432/yourdb"
export AG_PAYROLL_CSV_PATH="$(pwd)/data/payroll.csv"

./start-all.sh
```

| Service | URL |
|---------|-----|
| Reasoning API | `http://127.0.0.1:8787` |
| MCP (Cursor / Claude) | `http://127.0.0.1:3334/mcp` |

`start-all.sh` stops any existing processes on those ports, then starts both services. Press Ctrl+C to stop.

Default environment (override as needed):

| Variable | Default |
|----------|---------|
| `AG_PAYROLL_CSV_PATH` | `./data/payroll.csv` |
| `AG_REASONING_URL` | `http://127.0.0.1:8787` |
| `AG_MCP_PORT` | `3334` |

### Sample Postgres schema (CRM demos)

```sql
CREATE TABLE users (
  user_id   TEXT PRIMARY KEY,
  full_name TEXT NOT NULL
);

CREATE TABLE accounts (
  account_name  TEXT PRIMARY KEY,
  industry      TEXT,
  owner_user_id TEXT NOT NULL REFERENCES users(user_id)
);

INSERT INTO users VALUES ('alex.ae', 'Alex Anderson');
INSERT INTO accounts VALUES
  ('Northwind Traders', 'Retail', 'alex.ae'),
  ('Contoso Ltd', 'Technology', 'alex.ae');
```

Payroll sample data: `data/payroll.csv` (column `user` links to `users.user_id`).

### Validate playbooks

```bash
cargo run -p anythinggraph-ag -- validate --playbooks playbooks
```

### Add a new playbook

1. Create `playbooks/your-playbook.json` — entities, relationships, `entity_sources`, `bindings` map.
2. Add binding files under `bindings/` (e.g. `your-playbook.postgres.yaml`).
3. Validate and test via MCP `query_graph`.

Templates: `playbooks/simple-crm-access.json`, `playbooks/crm-payroll-access.json`.

### Add or edit a binding

Bindings live in `bindings/`. Each file maps playbook entities to physical tables or files.

- Postgres: `bindings/crm-payroll-access.postgres.yaml`
- CSV: `bindings/crm-payroll-access.csv.yaml` (maps playbook `user_id` → CSV column `user`)

Use MCP `propose_binding` and `test_binding` before `save_binding`, or edit YAML and restart `./start-all.sh`.

Credentials: `profiles/local.yaml` and env vars (`AG_SQL_DSN`, `AG_PAYROLL_CSV_PATH`, `AG_SF_*`).

### Full MCP tool list

| Tool | Purpose |
|------|---------|
| `health_check` | Service status |
| `get_playbook_context` | Entities, relationships, bindings map |
| `query_graph` | Ask a question (plan + execute + proof) |
| `list_sources` / `introspect_source` | Discover connected systems |
| `list_bindings` / `get_binding` | View mappings |
| `suggest_bindings` / `propose_binding` / `test_binding` / `save_binding` | Agent-driven onboarding |
| `plan_query` / `execute_plan` | Advanced: split plan and execution |

### Manual start (alternative)

```bash
# Terminal 1
export AG_SQL_DSN="postgres://..."
cargo run -p reasoning-service

# Terminal 2
cd mcp && AG_REASONING_URL=http://127.0.0.1:8787 npm run dev:http
```

### Environment reference

| Variable | Default | Purpose |
|----------|---------|---------|
| `AG_PLAYBOOKS_DIR` | `./playbooks` | Playbook JSON catalog |
| `AG_BINDINGS_DIR` | `./bindings` | Binding YAML files |
| `AG_PROFILE_PATH` | `./profiles/local.yaml` | Source credentials |
| `AG_SQL_DSN` | — | Postgres connection |
| `AG_SF_INSTANCE_URL` / `AG_SF_ACCESS_TOKEN` | — | Salesforce |
| `AG_PAYROLL_CSV_PATH` | — | CSV file path |
| `AG_REASONING_URL` | `http://127.0.0.1:8787` | MCP → reasoning API |
| `AG_MCP_PORT` | `3334` | MCP HTTP port |

### HTTP API

Reasoning service exposes `/health`, `/playbooks/{id}/context`, `/query`, binding onboarding endpoints, and more on port **8787**. See source in `reasoning-service/` if you need direct HTTP integration.

---

## License

Apache-2.0
