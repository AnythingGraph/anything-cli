# Anything Graph - Policy As Code Layer

**Anything Graph** is a semantic data layer for your AI agents. It addresses the most frustrating questions in agentic AI today:

```
🤝 How do I make agent answers consistent across teams?
🏢 How do I make agents work across different departments?
🔍 How do I improve retrieval quality in RAG?
🔒 How do I enforce permissions and access control?
📋 How do I improve auditability?
🎯 How do I avoid hallucination?
💸 How do I let my agent read millions of records without burning lots of tokens?
📖 How do I stop the agent from using the wrong definition of a term?
🔗 How do I make the agent understand data across many systems?
```

**We believe the answer is not another LLM or another AI orchestration system — it is your data.** You need a lightweight semantic layer between your AI agents and your systems of record. It never moves your data, is not another ETL project, and does not rely on dumping prompts.

Anything CLI

Your data stays where it is — Postgres, Salesforce, CSV files, and more. AnythingGraph gives Cursor, Claude, and other agents a **governed way to understand and query** that data, with answers you can trace back to the source.

---

## Why agents and AI developers love this

1. **Stable vocabulary** — “customer” and “owns_account” don’t change when IT renames a column (update the binding, not every prompt).
2. **Federated** — one playbook, many systems; no forcing everything into one database.
3. **Onboarding loop** — your agent can read your schema, propose a binding, test it, and save — you stay in the loop while the agent does the tedious mapping.
4. **Policy as code**  — queries go through a governed path (playbook → plan → source), not raw SQL from chat.

---

## How it works

### 1. Quick installation

```bash
npm install -g @anythinggraph/cli@latest
anythinggraph onboard --install-daemon
anythinggraph start
```

### 2. Set up your data connections

```bash
anythinggraph start
anythinggraph source add
```

`source add` is a four-step wizard: pick adapter (Postgres, MySQL, SQL Server, MongoDB, Salesforce, CSV, REST), choose a profile name (`source_id`), enter credentials, then validate before saving `profiles/local.yaml` and `.env`.

Git-clone contributors can use the same flow after `npm install -g @anythinggraph/cli`, or edit files manually:

```bash
cp .env.example .env
# edit profiles/local.yaml and .env, then:
chmod +x start-all.sh   # first time only
./start-all.sh
```

We support SQL databases, MongoDB, Salesforce SOQL, CSV, and REST/HTTP JSON APIs as data sources. See [connect your data](https://anythinggraph.com/connect-data.html) and the [full documentation](https://www.anythinggraph.com/documentation.html).

### 3. Wire up MCP in your favorite AI agent

```json
{
  "mcpServers": {
    "anythinggraph-thin": {
      "url": "http://127.0.0.1:3334/mcp"
    }
  }
}
```

Use [AGENTS.md](https://github.com/AnythingGraph/anything-cli/blob/main/AGENTS.md) in your agent workspace — it lists MCP tools and the compact playbook/binding format.

### 4. Browse your data and create an ontology playbook in natural language

This step has the most impact. Use your AI agent to browse data, define entities and relationships, and set role-based access control — all in natural language.

See the playbook guide: [anythingcli-playbooks-guide.html](https://www.anythinggraph.com/anythingcli-playbooks-guide.html).

The Anything Graph engine does most of the heavy lifting to produce your [ontology playbook and data bindings](https://www.anythinggraph.com/anythingcli-playbooks-guide.html).

### 5. Launch your ontology playbooks to production

Production playbooks provide scoped data access, AI reasoning, and governance. Example prompts:

- *Who is the person or role?*
- *What records are they responsible for?*
- *What related data may they see?*
- *How many accounts does Alex own?*
- *Show Alex’s payroll history count*
- *How many payroll records does Alex Anderson have?*

---

## Data adapters

Adapters connect profiles to live systems. **Seven ship today**; more share the same playbook and binding model.

[See how to connect your data sources](https://anythinggraph.com/connect-data.html)

| Adapter        | Profile key     | Typical source                  | Status        |
| -------------- | --------------- | ------------------------------- | ------------- |
| SQL            | `sql`           | PostgreSQL                      | **Available** |
| CSV            | `csv`           | Local CSV / flat files          | **Available** |
| SOQL           | `soql`          | Salesforce                      | **Available** |
| MySQL          | `mysql`         | MySQL, MariaDB                  | **Available** |
| SQL Server     | `mssql`         | Microsoft SQL Server, Azure SQL | **Available** |
| MongoDB        | `mongodb`       | MongoDB collections             | **Available** |
| REST / OpenAPI | `rest`          | HTTP JSON APIs                  | **Available** |
| BigQuery       | `bigquery`      | Google BigQuery                 | Planned       |
| Snowflake      | `snowflake`     | Snowflake                       | Planned       |
| Databricks     | `databricks`    | Databricks SQL                  | Planned       |
| Elasticsearch  | `elasticsearch` | Elasticsearch / OpenSearch      | Planned       |
| S3 / Parquet   | `s3`            | Object storage, Parquet files   | Planned       |
| GraphQL        | `graphql`       | GraphQL endpoints               | Planned       |
| Google Sheets  | `google_sheets` | Spreadsheets                    | Planned       |
| HubSpot        | `hubspot`       | HubSpot CRM                     | Planned       |

---

## Playbooks and bindings

How to author playbook JSON and binding YAML: **[playbooks/README.md](playbooks/README.md)** — or the web walkthrough at [anythingcli-playbooks-guide.html](https://www.anythinggraph.com/anythingcli-playbooks-guide.html) (uses demo playbooks as examples).

---

## Technical reference

### Install & run

**Requirements:** Rust, Node.js, Postgres (for CRM demos), optional CSV for payroll demo.

```bash
cargo build --release
cd mcp && npm install && cd ..

cp .env.example .env
# edit .env with your connection strings

./start-all.sh
```


| Service               | URL                         |
| --------------------- | --------------------------- |
| Reasoning API         | `http://127.0.0.1:8787`     |
| MCP (Cursor / Claude) | `http://127.0.0.1:3334/mcp` |


`start-all.sh` stops any existing processes on those ports, then starts both services. Press Ctrl+C to stop.

`start-all.sh` sets workspace paths and service URLs automatically. Override any value in `.env` — see [Environment reference](#environment-reference) below.

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

See **[playbooks/README.md](playbooks/README.md)** for the full authoring walkthrough.

### Credentials and profiles

1. `**cp .env.example .env`** — put secrets in `.env` (gitignored). `start-all.sh` loads it automatically.
2. `**profiles/local.yaml**` — registers named sources with `env:AG_*` references (no secrets in this file).

See `.env.example` for all supported variables. Details in the playbooks guide.

### Auth roles (one MCP + bearer token)

Local dev: `start-all.sh` sets `AG_AUTH_DISABLED=1` by default (no bearer token required). To enable auth, set in `.env`:

```bash
AG_AUTH_DISABLED=0
AG_ADMIN_TOKENS=admin-secret-change-me
AG_USER_TOKENS=user-secret-change-me
```

Clients send `Authorization: Bearer <token>` on MCP HTTP requests. The token maps to:


| Role      | MCP tools                                                                                                                                                                                                                                     | Reasoning HTTP                                                   |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| **user**  | `health_check`, `list_playbooks`, `get_playbook_context`, `list_entity`, `sample_entity`, `plan_query`, `execute_plan`, `query_graph`, `list_allowed_rows`                                                                                    | `/query`, `/plan`, `/execute`, `/rebac/`*, read playbook context |
| **admin** | All user tools **plus** `list_sources`, `get_adapter_guide`, `list_bindings`, `get_binding`, `introspect_source`, `sample_source`, `suggest_bindings`, `propose_binding`, `test_binding`, `save_binding`, `propose_playbook`, `save_playbook` | All endpoints                                                    |


When `AG_AUTH_DISABLED=1` or no tokens are configured, auth is disabled (local dev only).

**Profiles are never written via MCP** — edit `profiles/local.yaml` manually. Admin agents use `list_sources` → `get_adapter_guide(source_id)` → `introspect_source` before authoring bindings.

**Live data is read-only** — bindings reject non-SELECT queries; MCP has no insert/update/delete tools.

**Agent authoring:** see **[AGENTS.md](AGENTS.md)** for compact playbook/binding format. MCP `propose_binding` no longer returns expanded SQL by default — `save_binding` writes your submitted YAML **verbatim** (validation compiles in memory only).

Stdio MCP: set `AG_MCP_AUTH_TOKEN` to an admin or user token from the lists above.

### Full MCP tool list


| Tool                   | Role  | Purpose                                                                                  |
| ---------------------- | ----- | ---------------------------------------------------------------------------------------- |
| `health_check`         | user  | Ping Rust reasoning-service                                                              |
| `list_playbooks`       | user  | List playbook ids loaded from `playbooks/`                                               |
| `get_playbook_context` | user  | Load playbook schema summary (entities and relationships)                                |
| `list_entity`          | user  | List rows for a playbook entity (bounded browse; default limit 1000)                     |
| `sample_entity`        | user  | Return a small sample of rows for a playbook entity (default limit 5)                    |
| `plan_query`           | user  | Compile a structured federated query into plan IR                                        |
| `execute_plan`         | user  | Execute a compiled plan IR via read-only adapters                                        |
| `query_graph`          | user  | Compile and execute a federated read query in one step (proof envelope)                  |
| `list_allowed_rows`    | user  | List row identifiers a subject may read under enforced ReBAC rules                       |
| `list_sources`         | admin | List configured data sources from `profiles/local.yaml` (no secrets)                     |
| `get_adapter_guide`    | admin | Per-adapter binding authoring guide for a `source_id` — call before `propose_binding`    |
| `list_bindings`        | admin | List loaded binding file stems in `bindings/`                                            |
| `get_binding`          | admin | Load one saved binding YAML by stem                                                      |
| `introspect_source`    | admin | Read source schema for agent mapping (tables/columns — read-only)                        |
| `sample_source`        | admin | Read a few raw rows from a source table/collection/object (no playbook required)         |
| `suggest_bindings`     | admin | Suggest playbook entity-to-table mappings from introspected schema                       |
| `propose_binding`      | admin | Validate declarative binding YAML (no SQL)                                               |
| `test_binding`         | admin | Compile a sample query against a proposed or saved binding; optionally execute read-only |
| `save_binding`         | admin | Save declarative binding YAML to `bindings/{playbook_id}.{adapter_suffix}.yaml`          |
| `propose_playbook`     | admin | Validate compact playbook JSON                                                           |
| `save_playbook`        | admin | Save compact playbook JSON to `playbooks/{playbook_id}.json`                             |


### Manual start (alternative)

```bash
# Terminal 1
export AG_SQL_DSN="postgres://..."
export AG_ADMIN_TOKENS="admin-secret"
export AG_USER_TOKENS="user-secret"
cargo run -p reasoning-service

# Terminal 2
cd mcp && AG_REASONING_URL=http://127.0.0.1:8787 AG_ADMIN_TOKENS=admin-secret AG_USER_TOKENS=user-secret npm run dev:http
```

### Environment reference

Copy `.env.example` to `.env` and edit locally — never commit `.env`. `profiles/local.yaml` references credentials as `env:AG_*`; keep secrets in `.env` only.

**Data source credentials** (used by `profiles/local.yaml`):


| Variable              | Example / default                            | Purpose                                       |
| --------------------- | -------------------------------------------- | --------------------------------------------- |
| `AG_SQL_DSN`          | `postgres://user:pass@localhost:5432/yourdb` | Postgres — profile key `warehouse_pg`         |
| `AG_PAYROLL_CSV_PATH` | `./data/payroll.csv`                         | Local CSV — profile key `payroll_csv`         |
| `AG_MONGODB_DSN`      | `mongodb://localhost:27017`                  | MongoDB connection — profile key `mongo_main` |
| `AG_MONGODB_DATABASE` | `mydb`                                       | MongoDB default database                      |
| `AG_SF_INSTANCE_URL`  | `https://your-instance.my.salesforce.com`    | Salesforce instance URL                       |
| `AG_SF_ACCESS_TOKEN`  | —                                            | Salesforce OAuth access token                 |
| `AG_REST_BASE_URL`    | `https://api.example.com`                    | REST/HTTP JSON API base URL (optional)        |
| `AG_REST_TOKEN`       | —                                            | REST API bearer token (optional)              |


**Paths and workspace** (`start-all.sh` sets `AG_WORKSPACE_ROOT` to the repo root):


| Variable            | Default                           | Purpose                                              |
| ------------------- | --------------------------------- | ---------------------------------------------------- |
| `AG_WORKSPACE_ROOT` | repo root                         | Workspace root for playbooks, bindings, and profiles |
| `AG_PLAYBOOKS_DIR`  | `{workspace}/playbooks`           | Playbook JSON catalog                                |
| `AG_BINDINGS_DIR`   | `{workspace}/bindings`            | Binding YAML files                                   |
| `AG_PROFILE_PATH`   | `{workspace}/profiles/local.yaml` | Named source profiles (references `env:AG_`*)        |


**Services** (optional — defaults work for local dev):


| Variable            | Default                 | Purpose                      |
| ------------------- | ----------------------- | ---------------------------- |
| `AG_REASONING_HOST` | `127.0.0.1`             | Reasoning-service bind host  |
| `AG_REASONING_PORT` | `8787`                  | Reasoning-service port       |
| `AG_REASONING_URL`  | `http://127.0.0.1:8787` | MCP → reasoning API base URL |
| `AG_MCP_HOST`       | `127.0.0.1`             | MCP HTTP bind host           |
| `AG_MCP_PORT`       | `3334`                  | MCP HTTP port                |


**Auth**:


| Variable            | Default               | Purpose                                |
| ------------------- | --------------------- | -------------------------------------- |
| `AG_AUTH_DISABLED`  | `1` in `start-all.sh` | Set to `0` to require bearer tokens    |
| `AG_ADMIN_TOKENS`   | —                     | Comma-separated admin bearer tokens    |
| `AG_USER_TOKENS`    | —                     | Comma-separated user bearer tokens     |
| `AG_MCP_AUTH_TOKEN` | —                     | Default token for stdio MCP (optional) |


**Debug**:


| Variable            | Default | Purpose                                                                                         |
| ------------------- | ------- | ----------------------------------------------------------------------------------------------- |
| `AG_DEBUG_COMPILED` | unset   | Set to `1` to include `debug_compiled_binding_yaml` in `propose_binding` responses (debug only) |


### HTTP API

Reasoning service exposes `/health`, `/playbooks/{id}/context`, `/playbooks/{id}/propose-playbook`, `/playbooks/{id}/save-playbook`, `/query`, binding onboarding endpoints, and more on port **8787**. Protected routes require `Authorization: Bearer <token>` when auth tokens are configured.

---

## License

Apache-2.0

Copyright (C) 2026 — EdwardDeBon, AnythingGraph.
