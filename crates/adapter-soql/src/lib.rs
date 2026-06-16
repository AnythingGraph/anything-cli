mod introspect;
mod sample;
mod authoring;

pub use introspect::introspect_salesforce_schema;
pub use sample::sample_salesforce_object;
pub use authoring::authoring_guide;

use adapter_core::{AdapterError, DataAdapter, ExecContext, ExecutionState, map_row_to_playbook_fields, row_map_to_json};
use async_trait::async_trait;
use binding_spec::PlaybookBinding;
use plan_ir::{EntityRef, PlanStep, StepResult};
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;

pub struct SoqlAdapter {
    http_client: Client,
}

impl SoqlAdapter {
    pub fn new() -> Self {
        Self {
            http_client: Client::new(),
        }
    }
}

impl Default for SoqlAdapter {
    fn default() -> Self {
        Self::new()
    }
}

// Parsed Salesforce query response (records plus totalSize for COUNT() queries).
struct SoqlQueryResult {
    records: Vec<Value>,
    total_size: u64,
}

#[async_trait]
impl DataAdapter for SoqlAdapter {
    fn adapter_type(&self) -> &'static str {
        "soql"
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
                let soql = apply_soql_parameters(query_template, by_value);
                let query_result = run_soql_query(context, &self.http_client, &soql).await?;
                let first_record = query_result.records.first().ok_or_else(|| {
                    AdapterError::Message(format!("no records for entity resolve: {entity}"))
                })?;
                let physical_map: HashMap<String, Value> = first_record
                    .as_object()
                    .map(|object| object.iter().map(|(key, value)| (key.clone(), value.clone())).collect())
                    .unwrap_or_default();
                let mapped_row = map_row_to_playbook_fields(physical_map, entity_binding);
                let id_value = entity_binding
                    .fields
                    .iter()
                    .find(|(_, physical_column)| *physical_column == &entity_binding.id_field)
                    .and_then(|(playbook_field, _)| mapped_row.get(playbook_field))
                    .or_else(|| mapped_row.get(&entity_binding.id_field))
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string())
                    .ok_or_else(|| AdapterError::Message("resolved record missing id field".into()))?;

                Ok(StepResult {
                    step_index,
                    op: "resolve_entity".into(),
                    entity_ref: Some(EntityRef {
                        entity: entity.clone(),
                        id_field: entity_binding.id_field.clone(),
                        id_value,
                        display_value: mapped_row
                            .get(by_field)
                            .and_then(|value| value.as_str())
                            .map(|value| value.to_string()),
                    }),
                    count: None,
                    rows: Some(query_result.records),
                    source_query: Some(soql),
                    adapter: Some(self.adapter_type().to_string()),
                })
            }
            PlanStep::CountForSubject { relationship, .. } => {
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
                let soql = query_template.replace(":subject_id", &escape_soql_literal(&subject.id_value));
                let query_result = run_soql_query(context, &self.http_client, &soql).await?;
                let count_value = query_result
                    .records
                    .first()
                    .and_then(|row| row.get("expr0"))
                    .and_then(|value| value.as_u64())
                    .or_else(|| {
                        query_result.records.first().and_then(|row| {
                            row.as_object().and_then(|object| {
                                object.values().next().and_then(|value| value.as_u64())
                            })
                        })
                    })
                    .unwrap_or(query_result.total_size);

                Ok(StepResult {
                    step_index,
                    op: "count_for_subject".into(),
                    entity_ref: None,
                    count: Some(count_value),
                    rows: Some(query_result.records),
                    source_query: Some(soql),
                    adapter: Some(self.adapter_type().to_string()),
                })
            }
            PlanStep::ListForSubject {
                relationship,
                limit,
                ..
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
                let soql = query_template
                    .replace(":subject_id", &escape_soql_literal(&subject.id_value))
                    .replace(":limit", &limit.to_string());
                let query_result = run_soql_query(context, &self.http_client, &soql).await?;

                Ok(StepResult {
                    step_index,
                    op: "list_for_subject".into(),
                    entity_ref: None,
                    count: None,
                    rows: Some(query_result.records),
                    source_query: Some(soql),
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
                let soql = query_template.replace(":limit", &limit.to_string());
                let query_result = run_soql_query(context, &self.http_client, &soql).await?;
                let mut mapped_rows = Vec::new();
                for record in query_result.records {
                    if let Some(row_object) = record.as_object() {
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
                    source_query: Some(soql),
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
                AdapterError::MissingOperation(format!(
                    "{entity_name}.operations.list_all (add declarative from/fields or explicit list_all SOQL)"
                ))
            })?;

        let query_result = run_soql_query(context, &self.http_client, query_template).await?;
        let mut mapped_rows = Vec::new();
        for row_value in query_result.records {
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

// Replace named SOQL parameters with escaped literals.
fn apply_soql_parameters(query_template: &str, parameter_value: &str) -> String {
    query_template
        .replace(":name", &escape_soql_literal(parameter_value))
        .replace(":identifier", &escape_soql_literal(parameter_value))
}

// Execute one SOQL query via Salesforce REST API.
async fn run_soql_query(
    context: &ExecContext,
    http_client: &Client,
    soql: &str,
) -> Result<SoqlQueryResult, AdapterError> {
    let instance_url = context
        .connection
        .instance_url
        .as_ref()
        .ok_or_else(|| AdapterError::Message("soql adapter requires instance_url".into()))?;
    let access_token = context
        .connection
        .auth
        .as_ref()
        .ok_or_else(|| AdapterError::Message("soql adapter requires auth token".into()))?;

    let url = format!(
        "{}/services/data/v59.0/query?q={}",
        instance_url.trim_end_matches('/'),
        urlencoding::encode(soql)
    );

    let response = http_client
        .get(url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|error| AdapterError::Message(format!("soql request failed: {error}")))?;

    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AdapterError::Message(format!("soql error: {body}")));
    }

    let payload: Value = response
        .json()
        .await
        .map_err(|error| AdapterError::Message(format!("soql json parse failed: {error}")))?;

    let records = payload
        .get("records")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let total_size = payload
        .get("totalSize")
        .and_then(|value| value.as_u64())
        .unwrap_or(records.len() as u64);

    Ok(SoqlQueryResult {
        records,
        total_size,
    })
}

// Escape a string literal for SOQL.
fn escape_soql_literal(raw_value: &str) -> String {
    format!("'{}'", raw_value.replace('\'', "\\'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_soql_parameters_replaces_name_and_identifier() {
        let query = "SELECT Id FROM User WHERE Id = :identifier OR Name = :name";
        let rendered = apply_soql_parameters(query, "005ABC");
        assert!(rendered.contains("'005ABC'"));
        assert!(!rendered.contains(":identifier"));
        assert!(!rendered.contains(":name"));
    }

    #[test]
    fn escape_soql_literal_quotes_apostrophes() {
        assert_eq!(escape_soql_literal("O'Brien"), "'O\\'Brien'");
    }
}
