use adapter_core::AdapterAuthoringGuide;

// Return agent authoring guide for Postgres (profile adapter: sql).
pub fn postgres_authoring_guide() -> AdapterAuthoringGuide {
    base_sql_guide(
        "sql",
        "postgres",
        "SQL table name",
        Some("Postgres schema name for introspect_source (default: public)".into()),
    )
}

// Return agent authoring guide for MySQL (profile adapter: mysql).
pub fn mysql_authoring_guide() -> AdapterAuthoringGuide {
    base_sql_guide(
        "mysql",
        "mysql",
        "SQL table name",
        Some("MySQL database/schema name for introspect_source".into()),
    )
}

// Return agent authoring guide for SQL Server (profile adapter: mssql).
pub fn mssql_authoring_guide() -> AdapterAuthoringGuide {
    base_sql_guide(
        "mssql",
        "mssql",
        "SQL table name",
        Some("SQL Server schema name for introspect_source (default: dbo)".into()),
    )
}

// Build SQL-family authoring guide shared by postgres/mysql/mssql adapters.
fn base_sql_guide(
    adapter: &str,
    binding_suffix: &str,
    from_meaning: &str,
    introspect_hint: Option<String>,
) -> AdapterAuthoringGuide {
    AdapterAuthoringGuide {
        adapter: adapter.into(),
        binding_file_suffix: binding_suffix.into(),
        entity_from_meaning: from_meaning.into(),
        introspect_schema_name: introspect_hint,
        forbidden_binding_keys: vec![
            "schema_name".into(),
            "adapter".into(),
            "playbook_id".into(),
            "version".into(),
            "dsn".into(),
            "lookup".into(),
            "operations".into(),
        ],
        allowed_top_level_keys: vec![
            "source_id".into(),
            "entities".into(),
            "relationships".into(),
        ],
        instructions_markdown: include_str!("../AGENTS.md").into(),
        example_binding_yaml: Some(include_str!("../examples/binding-postgres.yaml").into()),
        workflow_steps: vec![
            "get_adapter_guide(source_id)".into(),
            "introspect_source(source_id, schema_name optional)".into(),
            format!("propose_binding → test_binding(execute=true) → save_binding(adapter_suffix={binding_suffix})"),
        ],
    }
}
