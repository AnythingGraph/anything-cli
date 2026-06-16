use adapter_core::{AdapterError, DataAdapter, ExecContext, ExecutionState, map_row_to_playbook_fields, row_map_to_json};
use async_trait::async_trait;
use binding_spec::PlaybookBinding;
use plan_ir::{EntityRef, PlanStep, StepResult};
use serde_json::{json, Value};
use sqlx::{Column, Row};
use std::collections::HashMap;

// SQL dialect used when connecting and compiling placeholder queries.
#[derive(Debug, Clone, Copy)]
pub enum SqlDialect {
    Postgres,
    Mysql,
    Mssql,
}

pub struct GenericSqlAdapter {
    pub dialect: SqlDialect,
    pub adapter_type: &'static str,
}

#[async_trait]
impl DataAdapter for GenericSqlAdapter {
    fn adapter_type(&self) -> &'static str {
        self.adapter_type
    }

    async fn execute_step(
        &self,
        step_index: usize,
        step: &PlanStep,
        binding: &PlaybookBinding,
        context: &ExecContext,
        state: &ExecutionState,
    ) -> Result<StepResult, AdapterError> {
        match step {
            PlanStep::ResolveEntity {
                entity,
                by_field,
                by_value,
            } => {
                let entity_binding = binding.entities.get(entity).ok_or_else(|| {
                    AdapterError::MissingEntityBinding(entity.clone())
                })?;
                let lookup_key = if by_field.as_str() == entity_binding.id_field {
                    "by_identifier"
                } else {
                    "by_name"
                };
                let query_template = entity_binding.lookup.get(lookup_key).ok_or_else(|| {
                    AdapterError::MissingOperation(format!("{entity}.{lookup_key}"))
                })?;
                let query_text = bind_query_placeholders(query_template, by_value, None, None);

                let rows = run_sql_query(self.dialect, context, &query_text).await?;
                let first_row = rows.first().ok_or_else(|| {
                    AdapterError::Message(format!("no rows for entity resolve: {entity}"))
                })?;
                let id_value = first_row
                    .get(&entity_binding.id_field)
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string())
                    .ok_or_else(|| AdapterError::Message("resolved row missing id field".into()))?;

                Ok(StepResult {
                    step_index,
                    op: "resolve_entity".into(),
                    entity_ref: Some(EntityRef {
                        entity: entity.clone(),
                        id_field: entity_binding.id_field.clone(),
                        id_value: id_value.clone(),
                        display_value: first_row
                            .get(by_field)
                            .and_then(|value| value.as_str())
                            .map(|value| value.to_string()),
                    }),
                    count: None,
                    rows: Some(rows),
                    source_query: Some(query_text),
                    adapter: Some(self.adapter_type().to_string()),
                })
            }
            PlanStep::CountForSubject {
                relationship,
                object_entity: _,
            } => {
                let subject = state.current_subject.as_ref().ok_or_else(|| {
                    AdapterError::Message("count requires resolved subject".into())
                })?;
                let relationship_binding = binding.relationships.get(relationship).ok_or_else(|| {
                    AdapterError::MissingOperation(format!("relationship.{relationship}"))
                })?;
                let query_template = relationship_binding
                    .operations
                    .get("count_for_subject")
                    .ok_or_else(|| {
                        AdapterError::MissingOperation(format!(
                            "{relationship}.operations.count_for_subject"
                        ))
                    })?;
                let query_text =
                    bind_query_placeholders(query_template, &subject.id_value, None, None);
                let rows = run_sql_query(self.dialect, context, &query_text).await?;
                let count_value = extract_count_from_rows(&rows);

                Ok(StepResult {
                    step_index,
                    op: "count_for_subject".into(),
                    entity_ref: None,
                    count: Some(count_value),
                    rows: Some(rows),
                    source_query: Some(query_text),
                    adapter: Some(self.adapter_type().to_string()),
                })
            }
            PlanStep::ListForSubject {
                relationship,
                object_entity: _,
                limit,
            } => {
                let subject = state.current_subject.as_ref().ok_or_else(|| {
                    AdapterError::Message("list requires resolved subject".into())
                })?;
                let relationship_binding = binding.relationships.get(relationship).ok_or_else(|| {
                    AdapterError::MissingOperation(format!("relationship.{relationship}"))
                })?;
                let query_template = relationship_binding
                    .operations
                    .get("list_for_subject")
                    .ok_or_else(|| {
                        AdapterError::MissingOperation(format!(
                            "{relationship}.operations.list_for_subject"
                        ))
                    })?;
                let query_text = bind_query_placeholders(
                    query_template,
                    &subject.id_value,
                    Some(u64::from(*limit)),
                    None,
                );
                let rows = run_sql_query(self.dialect, context, &query_text).await?;

                Ok(StepResult {
                    step_index,
                    op: "list_for_subject".into(),
                    entity_ref: None,
                    count: None,
                    rows: Some(rows),
                    source_query: Some(query_text),
                    adapter: Some(self.adapter_type().to_string()),
                })
            }
            PlanStep::ListEntity {
                entity,
                limit,
                sample,
            } => {
                let entity_binding = binding.entities.get(entity).ok_or_else(|| {
                    AdapterError::MissingEntityBinding(entity.clone())
                })?;
                let query_template = entity_binding
                    .operations
                    .get("list_entity")
                    .ok_or_else(|| {
                        AdapterError::MissingOperation(format!("{entity}.operations.list_entity"))
                    })?;
                let query_text =
                    bind_query_placeholders(query_template, "", Some(u64::from(*limit)), None);
                let physical_rows = run_sql_query(self.dialect, context, &query_text).await?;
                let mut mapped_rows = Vec::new();
                for row_value in physical_rows {
                    if let Some(row_object) = row_value.as_object() {
                        let physical_map: HashMap<String, Value> = row_object
                            .iter()
                            .map(|(key, value)| (key.clone(), value.clone()))
                            .collect();
                        mapped_rows.push(row_map_to_json(map_row_to_playbook_fields(
                            physical_map,
                            entity_binding,
                        )));
                    }
                }
                let row_count = mapped_rows.len() as u64;
                let op_name = if *sample {
                    "sample_entity".to_string()
                } else {
                    "list_entity".to_string()
                };

                Ok(StepResult {
                    step_index,
                    op: op_name,
                    entity_ref: None,
                    count: Some(row_count),
                    rows: Some(mapped_rows),
                    source_query: Some(query_text),
                    adapter: Some(self.adapter_type().to_string()),
                })
            }
        }
    }

    async fn load_entity_rows(
        &self,
        entity_name: &str,
        binding: &PlaybookBinding,
        context: &ExecContext,
    ) -> Result<Vec<Value>, AdapterError> {
        let entity_binding = binding.entities.get(entity_name).ok_or_else(|| {
            AdapterError::MissingEntityBinding(entity_name.to_string())
        })?;

        let query_template = entity_binding
            .operations
            .get("list_all")
            .ok_or_else(|| {
                AdapterError::MissingOperation(format!("{entity_name}.operations.list_all"))
            })?;

        let physical_rows = run_sql_query(self.dialect, context, query_template).await?;
        let mut mapped_rows = Vec::new();
        for row_value in physical_rows {
            if let Some(row_object) = row_value.as_object() {
                let physical_map: HashMap<String, Value> = row_object
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect();
                mapped_rows.push(row_map_to_json(map_row_to_playbook_fields(
                    physical_map,
                    entity_binding,
                )));
            }
        }
        Ok(mapped_rows)
    }
}

// Replace :name, :identifier, :subject_id, and :limit placeholders in SQL templates.
pub fn bind_query_placeholders(
    query_template: &str,
    primary_value: &str,
    limit: Option<u64>,
    name_value: Option<&str>,
) -> String {
    let escaped_primary = escape_sql_string(primary_value);
    let escaped_name = escape_sql_string(name_value.unwrap_or(primary_value));
    let mut query_text = query_template.replace(":identifier", &format!("'{escaped_primary}'"));
    query_text = query_text.replace(":subject_id", &format!("'{escaped_primary}'"));
    query_text = query_text.replace(":name", &format!("'{escaped_name}'"));
    if let Some(limit_value) = limit {
        query_text = query_text.replace(":limit", &limit_value.to_string());
    }
    query_text
}

fn escape_sql_string(raw_value: &str) -> String {
    raw_value.replace('\'', "''")
}

fn extract_count_from_rows(rows: &[Value]) -> u64 {
    rows.first()
        .and_then(|row| row.get("count"))
        .and_then(|value| value.as_u64())
        .or_else(|| {
            rows.first()
                .and_then(|row| row.get("count"))
                .and_then(|value| value.as_i64())
                .map(|value| value as u64)
        })
        .or_else(|| {
            rows.first()
                .and_then(|row| row.get("count"))
                .and_then(|value| value.as_str())
                .and_then(|text| text.parse::<u64>().ok())
        })
        .unwrap_or(0)
}

// Run one SQL query using the configured DSN for the given dialect.
pub async fn run_sql_query(
    dialect: SqlDialect,
    context: &ExecContext,
    query_text: &str,
) -> Result<Vec<Value>, AdapterError> {
    let dsn = context
        .connection
        .dsn
        .as_ref()
        .ok_or_else(|| AdapterError::Message("sql adapter requires dsn in profile".into()))?;

    run_sql_query_on_dsn(dialect, dsn, query_text).await
}

// Run one SQL query against a DSN (used for source-level sampling without ExecContext).
pub async fn run_sql_query_on_dsn(
    dialect: SqlDialect,
    dsn: &str,
    query_text: &str,
) -> Result<Vec<Value>, AdapterError> {
    match dialect {
        SqlDialect::Postgres => run_postgres_query(dsn, query_text).await,
        SqlDialect::Mysql => run_mysql_query(dsn, query_text).await,
        SqlDialect::Mssql => run_mssql_query(dsn, query_text).await,
    }
}

async fn run_postgres_query(dsn: &str, query_text: &str) -> Result<Vec<Value>, AdapterError> {
    use sqlx::postgres::PgPoolOptions;

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(dsn)
        .await
        .map_err(|error| AdapterError::Message(format!("postgres connect failed: {error}")))?;

    let rows = sqlx::query(query_text)
        .fetch_all(&pool)
        .await
        .map_err(|error| AdapterError::Message(format!("postgres query failed: {error}")))?;

    let mut json_rows = Vec::new();
    for row in rows {
        let mut map = HashMap::new();
        for column in row.columns() {
            let column_name = column.name().to_string();
            let value: Option<String> = row.try_get(column_name.as_str()).ok();
            map.insert(column_name, json!(value));
        }
        json_rows.push(row_map_to_json(map));
    }
    Ok(json_rows)
}

async fn run_mysql_query(dsn: &str, query_text: &str) -> Result<Vec<Value>, AdapterError> {
    use sqlx::mysql::MySqlPoolOptions;

    let pool = MySqlPoolOptions::new()
        .max_connections(2)
        .connect(dsn)
        .await
        .map_err(|error| AdapterError::Message(format!("mysql connect failed: {error}")))?;

    let rows = sqlx::query(query_text)
        .fetch_all(&pool)
        .await
        .map_err(|error| AdapterError::Message(format!("mysql query failed: {error}")))?;

    let mut json_rows = Vec::new();
    for row in rows {
        let mut map = HashMap::new();
        for column in row.columns() {
            let column_name = column.name().to_string();
            let value: Option<String> = row.try_get(column_name.as_str()).ok();
            map.insert(column_name, json!(value));
        }
        json_rows.push(row_map_to_json(map));
    }
    Ok(json_rows)
}

async fn run_mssql_query(dsn: &str, query_text: &str) -> Result<Vec<Value>, AdapterError> {
    use futures_util::TryStreamExt;
    use tiberius::{Client, Config, QueryItem};
    use tokio::net::TcpStream;
    use tokio_util::compat::TokioAsyncWriteCompatExt;

    let config = Config::from_jdbc_string(dsn).map_err(|error| {
        AdapterError::Message(format!("mssql parse dsn failed: {error}"))
    })?;

    let tcp = TcpStream::connect(config.get_addr())
        .await
        .map_err(|error| AdapterError::Message(format!("mssql connect failed: {error}")))?;
    tcp.set_nodelay(true)
        .map_err(|error| AdapterError::Message(format!("mssql tcp config failed: {error}")))?;

    let mut client = Client::connect(config, tcp.compat_write())
        .await
        .map_err(|error| AdapterError::Message(format!("mssql client connect failed: {error}")))?;

    let mut stream = client
        .query(query_text, &[])
        .await
        .map_err(|error| AdapterError::Message(format!("mssql query failed: {error}")))?;

    let mut json_rows = Vec::new();
    while let Some(item) = stream
        .try_next()
        .await
        .map_err(|error| AdapterError::Message(format!("mssql result stream failed: {error}")))?
    {
        if let QueryItem::Row(row) = item {
            let mut map = HashMap::new();
            for index in 0..row.columns().len() {
                let column = row.columns()[index].name().to_string();
                let value: Option<String> = row.try_get::<&str, _>(index).ok().flatten().map(|text| text.to_string());
                map.insert(column, json!(value));
            }
            json_rows.push(row_map_to_json(map));
        }
    }

    Ok(json_rows)
}
