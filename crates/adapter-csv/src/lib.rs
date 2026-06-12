use std::collections::HashMap;
use std::path::{Path, PathBuf};

use adapter_core::{AdapterError, DataAdapter, ExecContext, ExecutionState, row_map_to_json};
use async_trait::async_trait;
use binding_spec::PlaybookBinding;
use csv::ReaderBuilder;
use plan_ir::{EntityRef, PlanStep, StepResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub struct CsvAdapter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsvSchemaCatalog {
    pub adapter: String,
    pub file_path: String,
    pub columns: Vec<String>,
    pub row_count: usize,
}

#[async_trait]
impl DataAdapter for CsvAdapter {
    fn adapter_type(&self) -> &'static str {
        "csv"
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
                let file_path = resolve_csv_file_path(context, entity_binding.from.as_deref())?;
                let rows = load_csv_rows(&file_path)?;
                let matched_row = rows.into_iter().find(|row| {
                    row_matches_resolve(row, entity_binding, by_field, by_value)
                }).ok_or_else(|| {
                    AdapterError::Message(format!("no csv rows for entity resolve: {entity}"))
                })?;

                let physical_id_column =
                    physical_column_for_field(entity_binding, &entity_binding.id_field);
                let id_value = matched_row
                    .get(&physical_id_column)
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string())
                    .ok_or_else(|| AdapterError::Message("csv row missing id field".into()))?;

                let display_column = physical_column_for_field(entity_binding, by_field);

                Ok(StepResult {
                    step_index,
                    op: "resolve_entity".into(),
                    entity_ref: Some(EntityRef {
                        entity: entity.clone(),
                        id_field: entity_binding.id_field.clone(),
                        id_value: id_value.clone(),
                        display_value: matched_row
                            .get(&display_column)
                            .and_then(|value| value.as_str())
                            .map(|value| value.to_string()),
                    }),
                    count: None,
                    rows: Some(vec![matched_row]),
                    source_query: Some(format!(
                        "csv:{} filter {by_field}={by_value}",
                        file_path.display()
                    )),
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
                let link_column = relationship_binding
                    .subject_link_column
                    .as_ref()
                    .ok_or_else(|| {
                        AdapterError::MissingOperation(format!(
                            "{relationship}.subject_link_column"
                        ))
                    })?;

                let object_entity = relationship_binding
                    .join
                    .as_ref()
                    .map(|join| join.to_entity.clone())
                    .ok_or_else(|| {
                        AdapterError::MissingOperation(format!("{relationship}.join.to_entity"))
                    })?;
                let object_binding = binding.entities.get(&object_entity).ok_or_else(|| {
                    AdapterError::MissingEntityBinding(object_entity.clone())
                })?;
                let file_path = resolve_csv_file_path(context, object_binding.from.as_deref())?;
                let rows = load_csv_rows(&file_path)?;
                let filtered_rows: Vec<Value> = rows
                    .into_iter()
                    .filter(|row| row_link_matches(row, link_column, &subject.id_value))
                    .collect();
                let count_value = filtered_rows.len() as u64;

                Ok(StepResult {
                    step_index,
                    op: "count_for_subject".into(),
                    entity_ref: None,
                    count: Some(count_value),
                    rows: Some(filtered_rows),
                    source_query: Some(format!(
                        "csv:{} filter {link_column}={}",
                        file_path.display(),
                        subject.id_value
                    )),
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
                let link_column = relationship_binding
                    .subject_link_column
                    .as_ref()
                    .ok_or_else(|| {
                        AdapterError::MissingOperation(format!(
                            "{relationship}.subject_link_column"
                        ))
                    })?;

                let object_entity = relationship_binding
                    .join
                    .as_ref()
                    .map(|join| join.to_entity.clone())
                    .ok_or_else(|| {
                        AdapterError::MissingOperation(format!("{relationship}.join.to_entity"))
                    })?;
                let object_binding = binding.entities.get(&object_entity).ok_or_else(|| {
                    AdapterError::MissingEntityBinding(object_entity.clone())
                })?;
                let file_path = resolve_csv_file_path(context, object_binding.from.as_deref())?;
                let rows = load_csv_rows(&file_path)?;
                let filtered_rows: Vec<Value> = rows
                    .into_iter()
                    .filter(|row| row_link_matches(row, link_column, &subject.id_value))
                    .take(*limit as usize)
                    .collect();

                Ok(StepResult {
                    step_index,
                    op: "list_for_subject".into(),
                    entity_ref: None,
                    count: None,
                    rows: Some(filtered_rows),
                    source_query: Some(format!(
                        "csv:{} filter {link_column}={} limit {limit}",
                        file_path.display(),
                        subject.id_value
                    )),
                    adapter: Some(self.adapter_type().to_string()),
                })
            }
        }
    }
}

// Read CSV schema metadata for agent onboarding.
pub fn introspect_csv_file(file_path: &Path) -> Result<CsvSchemaCatalog, AdapterError> {
    let rows = load_csv_rows(file_path)?;
    let columns = rows
        .first()
        .map(|row| {
            row.as_object()
                .map(|object| object.keys().cloned().collect())
                .unwrap_or_default()
        })
        .unwrap_or_default();

    Ok(CsvSchemaCatalog {
        adapter: "csv".into(),
        file_path: file_path.to_string_lossy().to_string(),
        columns,
        row_count: rows.len(),
    })
}

// Resolve CSV path from profile connection and optional entity file name.
fn resolve_csv_file_path(
    context: &ExecContext,
    entity_file_name: Option<&str>,
) -> Result<PathBuf, AdapterError> {
    let profile_path = context
        .connection
        .file_path
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AdapterError::Message("csv adapter requires file_path in profile".into()))?;

    if let Some(file_name) = entity_file_name.filter(|value| !value.trim().is_empty()) {
        let profile_parent = Path::new(profile_path)
            .parent()
            .unwrap_or_else(|| Path::new("."));
        return Ok(profile_parent.join(file_name));
    }

    Ok(PathBuf::from(profile_path))
}

// Load all CSV rows as JSON objects keyed by column header.
fn load_csv_rows(file_path: &Path) -> Result<Vec<Value>, AdapterError> {
    let raw_text = std::fs::read_to_string(file_path).map_err(|error| {
        AdapterError::Message(format!("read csv failed ({}): {error}", file_path.display()))
    })?;

    let mut reader = ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(raw_text.as_bytes());

    let headers = reader
        .headers()
        .map_err(|error| AdapterError::Message(format!("csv headers failed: {error}")))?
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();

    let mut rows = Vec::new();
    for record_result in reader.records() {
        let record = record_result
            .map_err(|error| AdapterError::Message(format!("csv row parse failed: {error}")))?;
        let mut map = HashMap::new();
        for (header, field_value) in headers.iter().zip(record.iter()) {
            map.insert(header.clone(), json!(field_value));
        }
        rows.push(row_map_to_json(map));
    }

    Ok(rows)
}

// Map playbook field name to physical CSV column via binding fields map.
fn physical_column_for_field(
    entity_binding: &binding_spec::EntityBinding,
    playbook_field: &str,
) -> String {
    entity_binding
        .fields
        .get(playbook_field)
        .cloned()
        .unwrap_or_else(|| playbook_field.to_string())
}

// Match one CSV row for resolve-entity step.
fn row_matches_resolve(
    row: &Value,
    entity_binding: &binding_spec::EntityBinding,
    by_field: &str,
    by_value: &str,
) -> bool {
    let column_name = physical_column_for_field(entity_binding, by_field);

    row.get(&column_name)
        .and_then(|value| value.as_str())
        .map(|value| value.eq_ignore_ascii_case(by_value))
        .unwrap_or(false)
}

// Match relationship link column to subject id.
fn row_link_matches(row: &Value, link_column: &str, subject_id: &str) -> bool {
    row.get(link_column)
        .and_then(|value| value.as_str())
        .map(|value| value.eq_ignore_ascii_case(subject_id))
        .unwrap_or(false)
}
