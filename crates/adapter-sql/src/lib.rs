use adapter_core::{AdapterError, DataAdapter, ExecContext, ExecutionState, map_row_to_playbook_fields, row_map_to_json};
use async_trait::async_trait;
use binding_spec::PlaybookBinding;
use plan_ir::{EntityRef, PlanStep, StepResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Column, Row};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSchemaCatalog {
    pub adapter: String,
    pub schema_name: String,
    pub tables: Vec<TableSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSchema {
    pub table_name: String,
    pub columns: Vec<ColumnSchema>,
    pub foreign_keys: Vec<ForeignKeySchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnSchema {
    pub column_name: String,
    pub data_type: String,
    pub is_nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKeySchema {
    pub column_name: String,
    pub foreign_table_name: String,
    pub foreign_column_name: String,
}

pub struct SqlAdapter;

#[async_trait]
impl DataAdapter for SqlAdapter {
    fn adapter_type(&self) -> &'static str {
        "sql"
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
                let lookup_key = if by_field == "full_name" {
                    "by_name"
                } else {
                    "by_identifier"
                };
                let query_template = entity_binding.lookup.get(lookup_key).ok_or_else(|| {
                    AdapterError::MissingOperation(format!("{entity}.{lookup_key}"))
                })?;
                let query_text = query_template.replace(":name", &format!("'{}'", by_value.replace('\'', "''")));
                let query_text = query_text.replace(":identifier", &format!("'{}'", by_value.replace('\'', "''")));

                let rows = run_sql_query(context, &query_text).await?;
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
                let query_text = query_template.replace(":subject_id", &format!("'{}'", subject.id_value.replace('\'', "''")));
                let rows = run_sql_query(context, &query_text).await?;
                let count_value = rows
                    .first()
                    .and_then(|row| row.get("count"))
                    .and_then(|value| value.as_u64())
                    .or_else(|| {
                        rows.first()
                            .and_then(|row| row.get("count"))
                            .and_then(|value| value.as_str())
                            .and_then(|text| text.parse::<u64>().ok())
                    })
                    .unwrap_or(0);

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
                let query_text = query_template
                    .replace(":subject_id", &format!("'{}'", subject.id_value.replace('\'', "''")))
                    .replace(":limit", &limit.to_string());
                let rows = run_sql_query(context, &query_text).await?;

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

        let physical_rows = run_sql_query(context, query_template).await?;
        let mut mapped_rows = Vec::new();
        for row_value in physical_rows {
            if let Some(row_object) = row_value.as_object() {
                let physical_map: HashMap<String, Value> =
                    row_object.iter().map(|(key, value)| (key.clone(), value.clone())).collect();
                mapped_rows.push(row_map_to_json(
                    map_row_to_playbook_fields(physical_map, entity_binding),
                ));
            }
        }
        Ok(mapped_rows)
    }
}

// Run one SQL query against the configured Postgres DSN.
async fn run_sql_query(
    context: &ExecContext,
    query_text: &str,
) -> Result<Vec<Value>, AdapterError> {
    let dsn = context
        .connection
        .dsn
        .as_ref()
        .ok_or_else(|| AdapterError::Message("sql adapter requires dsn in profile".into()))?;

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(dsn)
        .await
        .map_err(|error| AdapterError::Message(format!("sql connect failed: {error}")))?;

    let rows = sqlx::query(query_text)
        .fetch_all(&pool)
        .await
        .map_err(|error| AdapterError::Message(format!("sql query failed: {error}")))?;

    let mut json_rows = Vec::new();
    for row in rows {
        let mut map = HashMap::new();
        let columns = row.columns();
        for column in columns {
            let column_name = column.name().to_string();
            let value: Option<String> = row.try_get(column_name.as_str()).ok();
            map.insert(column_name, json!(value));
        }
        json_rows.push(row_map_to_json(map));
    }
    Ok(json_rows)
}

// Introspect Postgres tables, columns, and foreign keys for agent binding workflows.
pub async fn introspect_postgres_schema(
    dsn: &str,
    schema_name: Option<&str>,
) -> Result<SourceSchemaCatalog, AdapterError> {
    let schema_name = schema_name.unwrap_or("public");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(dsn)
        .await
        .map_err(|error| AdapterError::Message(format!("sql connect failed: {error}")))?;

    let column_rows = sqlx::query(
        "
        SELECT column_info.table_name, column_info.column_name, column_info.data_type, column_info.is_nullable
        FROM information_schema.columns AS column_info
        JOIN information_schema.tables AS table_info
          ON column_info.table_schema = table_info.table_schema
         AND column_info.table_name = table_info.table_name
        WHERE column_info.table_schema = $1
          AND table_info.table_type = 'BASE TABLE'
        ORDER BY column_info.table_name, column_info.ordinal_position
        ",
    )
    .bind(schema_name)
    .fetch_all(&pool)
    .await
    .map_err(|error| AdapterError::Message(format!("schema columns query failed: {error}")))?;

    let foreign_key_rows = sqlx::query(
        "
        SELECT
            source_table.table_name AS table_name,
            source_column.column_name AS column_name,
            target_table.table_name AS foreign_table_name,
            target_column.column_name AS foreign_column_name
        FROM information_schema.table_constraints AS table_constraint
        JOIN information_schema.key_column_usage AS source_column
          ON table_constraint.constraint_name = source_column.constraint_name
         AND table_constraint.table_schema = source_column.table_schema
        JOIN information_schema.constraint_column_usage AS target_column
          ON table_constraint.constraint_name = target_column.constraint_name
         AND table_constraint.table_schema = target_column.table_schema
        JOIN information_schema.tables AS source_table
          ON source_table.table_name = source_column.table_name
         AND source_table.table_schema = source_column.table_schema
        JOIN information_schema.tables AS target_table
          ON target_table.table_name = target_column.table_name
         AND target_table.table_schema = target_column.table_schema
        WHERE table_constraint.constraint_type = 'FOREIGN KEY'
          AND table_constraint.table_schema = $1
        ORDER BY source_table.table_name, source_column.column_name
        ",
    )
    .bind(schema_name)
    .fetch_all(&pool)
    .await
    .map_err(|error| AdapterError::Message(format!("schema foreign keys query failed: {error}")))?;

    let mut tables_by_name: HashMap<String, TableSchema> = HashMap::new();

    for row in column_rows {
        let table_name: String = row
            .try_get("table_name")
            .map_err(|error| AdapterError::Message(format!("read table_name failed: {error}")))?;
        let column_name: String = row
            .try_get("column_name")
            .map_err(|error| AdapterError::Message(format!("read column_name failed: {error}")))?;
        let data_type: String = row
            .try_get("data_type")
            .map_err(|error| AdapterError::Message(format!("read data_type failed: {error}")))?;
        let is_nullable: String = row
            .try_get("is_nullable")
            .map_err(|error| AdapterError::Message(format!("read is_nullable failed: {error}")))?;

        let table_entry = tables_by_name.entry(table_name.clone()).or_insert(TableSchema {
            table_name,
            columns: Vec::new(),
            foreign_keys: Vec::new(),
        });
        table_entry.columns.push(ColumnSchema {
            column_name,
            data_type,
            is_nullable: is_nullable.eq_ignore_ascii_case("YES"),
        });
    }

    for row in foreign_key_rows {
        let table_name: String = row
            .try_get("table_name")
            .map_err(|error| AdapterError::Message(format!("read table_name failed: {error}")))?;
        let column_name: String = row
            .try_get("column_name")
            .map_err(|error| AdapterError::Message(format!("read column_name failed: {error}")))?;
        let foreign_table_name: String = row.try_get("foreign_table_name").map_err(|error| {
            AdapterError::Message(format!("read foreign_table_name failed: {error}"))
        })?;
        let foreign_column_name: String = row.try_get("foreign_column_name").map_err(|error| {
            AdapterError::Message(format!("read foreign_column_name failed: {error}"))
        })?;

        let table_entry = tables_by_name.entry(table_name.clone()).or_insert(TableSchema {
            table_name,
            columns: Vec::new(),
            foreign_keys: Vec::new(),
        });
        table_entry.foreign_keys.push(ForeignKeySchema {
            column_name,
            foreign_table_name,
            foreign_column_name,
        });
    }

    let mut tables: Vec<TableSchema> = tables_by_name.into_values().collect();
    tables.sort_by(|left, right| left.table_name.cmp(&right.table_name));

    Ok(SourceSchemaCatalog {
        adapter: "sql".into(),
        schema_name: schema_name.to_string(),
        tables,
    })
}
