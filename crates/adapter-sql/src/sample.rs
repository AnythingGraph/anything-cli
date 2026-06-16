use adapter_core::AdapterError;
use serde_json::Value;

use crate::engine::{run_sql_query_on_dsn, SqlDialect};

pub const DEFAULT_SOURCE_SAMPLE_LIMIT: u32 = 5;
pub const MAX_SOURCE_SAMPLE_LIMIT: u32 = 100;

// Return up to `limit` raw rows from one SQL table (read-only discovery; no playbook).
pub async fn sample_sql_table(
    dialect: SqlDialect,
    dsn: &str,
    schema_name: Option<&str>,
    table_name: &str,
    limit: u32,
) -> Result<(String, Vec<Value>), AdapterError> {
    validate_sql_identifier(table_name)?;
    if let Some(schema) = schema_name {
        validate_sql_identifier(schema)?;
    }

    let capped_limit = cap_sample_limit(limit);
    let qualified_table = match schema_name {
        Some(schema) => format!("{schema}.{table_name}"),
        None => table_name.to_string(),
    };

    let query_text = format_sample_query(dialect, &qualified_table, capped_limit);
    let rows = run_sql_query_on_dsn(dialect, dsn, &query_text).await?;
    Ok((query_text, rows))
}

// Build dialect-specific bounded SELECT for source sampling.
fn format_sample_query(dialect: SqlDialect, qualified_table: &str, limit: u32) -> String {
    match dialect {
        SqlDialect::Mssql => format!("SELECT TOP ({limit}) * FROM {qualified_table}"),
        _ => format!("SELECT * FROM {qualified_table} LIMIT {limit}"),
    }
}

// Cap row sample size for safe discovery queries.
pub fn cap_sample_limit(limit: u32) -> u32 {
    let normalized = if limit == 0 { DEFAULT_SOURCE_SAMPLE_LIMIT } else { limit };
    normalized.min(MAX_SOURCE_SAMPLE_LIMIT)
}

// Reject identifiers that could enable SQL injection in table/schema names.
fn validate_sql_identifier(identifier: &str) -> Result<(), AdapterError> {
    if identifier.is_empty() {
        return Err(AdapterError::Message("table or schema name is required".into()));
    }
    if !identifier
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(AdapterError::Message(format!(
            "invalid SQL identifier '{identifier}' (use letters, numbers, underscore only)"
        )));
    }
    Ok(())
}
