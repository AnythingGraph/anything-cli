use std::collections::HashMap;

use adapter_core::{
    AdapterError, DataAdapter, ExecContext, ExecutionState, map_row_to_playbook_fields,
    row_map_to_json,
};
use async_trait::async_trait;
use binding_spec::PlaybookBinding;
use plan_ir::{EntityRef, PlanStep, StepResult};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub struct RestAdapter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestSchemaCatalog {
    pub adapter: String,
    pub base_url: String,
    pub resources: Vec<String>,
}

#[derive(Debug, Clone)]
struct RestOperation {
    method: String,
    path_and_query: String,
}

#[async_trait]
impl DataAdapter for RestAdapter {
    fn adapter_type(&self) -> &'static str {
        "rest"
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
                let operation_template = entity_binding.lookup.get(lookup_key).ok_or_else(|| {
                    AdapterError::MissingOperation(format!("{entity}.{lookup_key}"))
                })?;
                let operation = parse_rest_operation(operation_template)?;
                let rows = run_rest_request(context, &operation, by_value, None).await?;
                let first_row = rows.first().ok_or_else(|| {
                    AdapterError::Message(format!("no rest rows for entity resolve: {entity}"))
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
                        id_value,
                        display_value: first_row
                            .get(by_field)
                            .and_then(|value| value.as_str())
                            .map(|value| value.to_string()),
                    }),
                    count: None,
                    rows: Some(rows),
                    source_query: Some(format!("{} {}", operation.method, operation.path_and_query)),
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
                let operation_template = relationship_binding
                    .operations
                    .get("count_for_subject")
                    .ok_or_else(|| {
                        AdapterError::MissingOperation(format!(
                            "{relationship}.operations.count_for_subject"
                        ))
                    })?;
                let operation = parse_rest_operation(operation_template)?;
                let rows = run_rest_request(context, &operation, &subject.id_value, None).await?;
                let count_value = extract_rest_count(&rows);

                Ok(StepResult {
                    step_index,
                    op: "count_for_subject".into(),
                    entity_ref: None,
                    count: Some(count_value),
                    rows: Some(rows),
                    source_query: Some(format!("{} {}", operation.method, operation.path_and_query)),
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
                let operation_template = relationship_binding
                    .operations
                    .get("list_for_subject")
                    .ok_or_else(|| {
                        AdapterError::MissingOperation(format!(
                            "{relationship}.operations.list_for_subject"
                        ))
                    })?;
                let operation = parse_rest_operation(operation_template)?;
                let rows = run_rest_request(
                    context,
                    &operation,
                    &subject.id_value,
                    Some(u64::from(*limit)),
                )
                .await?;

                Ok(StepResult {
                    step_index,
                    op: "list_for_subject".into(),
                    entity_ref: None,
                    count: None,
                    rows: Some(rows),
                    source_query: Some(format!("{} {}", operation.method, operation.path_and_query)),
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
        let operation_template = entity_binding
            .operations
            .get("list_all")
            .ok_or_else(|| {
                AdapterError::MissingOperation(format!("{entity_name}.operations.list_all"))
            })?;
        let operation = parse_rest_operation(operation_template)?;
        let physical_rows = run_rest_request(context, &operation, "", None).await?;

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

// Parse REST operation strings such as GET /users?name=:name.
fn parse_rest_operation(raw_operation: &str) -> Result<RestOperation, AdapterError> {
    let mut parts = raw_operation.splitn(2, ' ');
    let method = parts
        .next()
        .ok_or_else(|| AdapterError::Message(format!("invalid rest operation: {raw_operation}")))?
        .trim()
        .to_ascii_uppercase();
    let path_and_query = parts
        .next()
        .ok_or_else(|| AdapterError::Message(format!("invalid rest operation: {raw_operation}")))?
        .trim()
        .to_string();

    Ok(RestOperation {
        method,
        path_and_query,
    })
}

// Execute HTTP request and normalize JSON payload into row objects.
async fn run_rest_request(
    context: &ExecContext,
    operation: &RestOperation,
    primary_value: &str,
    limit: Option<u64>,
) -> Result<Vec<Value>, AdapterError> {
    let base_url = context
        .connection
        .base_url
        .as_ref()
        .ok_or_else(|| AdapterError::Message("rest adapter requires base_url in profile".into()))?;

    let mut path_and_query = operation.path_and_query.clone();
    path_and_query = path_and_query.replace(":identifier", primary_value);
    path_and_query = path_and_query.replace(":subject_id", primary_value);
    path_and_query = path_and_query.replace(":name", primary_value);
    if let Some(limit_value) = limit {
        path_and_query = path_and_query.replace(":limit", &limit_value.to_string());
    }

    if path_and_query.contains(':') && !primary_value.is_empty() {
        let path_segments: Vec<String> = path_and_query
            .split('/')
            .map(|segment| segment.to_string())
            .collect();
        let mut rebuilt_segments = Vec::new();
        for segment in path_segments {
            if segment.starts_with(':') {
                let placeholder = segment.split('?').next().unwrap_or(&segment).to_string();
                rebuilt_segments.push(segment.replace(&placeholder, primary_value));
            } else {
                rebuilt_segments.push(segment);
            }
        }
        path_and_query = rebuilt_segments.join("/");
    }

    let request_url = join_base_url(base_url, &path_and_query);
    let client = Client::new();
    let mut request_builder = match operation.method.as_str() {
        "GET" => client.get(&request_url),
        "POST" => client.post(&request_url),
        _ => {
            return Err(AdapterError::Message(format!(
                "unsupported rest method: {}",
                operation.method
            )))
        }
    };

    request_builder = request_builder.header(CONTENT_TYPE, "application/json");
    if let Some(auth_token) = context.connection.auth.as_ref() {
        if auth_token.starts_with("Bearer ") {
            request_builder = request_builder.header(AUTHORIZATION, auth_token.as_str());
        } else {
            request_builder =
                request_builder.header(AUTHORIZATION, format!("Bearer {auth_token}"));
        }
    }

    let response = request_builder
        .send()
        .await
        .map_err(|error| AdapterError::Message(format!("rest request failed: {error}")))?;

    if !response.status().is_success() {
        return Err(AdapterError::Message(format!(
            "rest request returned status {}",
            response.status()
        )));
    }

    let payload: Value = response
        .json()
        .await
        .map_err(|error| AdapterError::Message(format!("rest response parse failed: {error}")))?;

    normalize_rest_payload(payload, limit)
}

// Join profile base URL with operation path.
fn join_base_url(base_url: &str, path_and_query: &str) -> String {
    let trimmed_base = base_url.trim_end_matches('/');
    if path_and_query.starts_with("http://") || path_and_query.starts_with("https://") {
        return path_and_query.to_string();
    }
    if path_and_query.starts_with('/') {
        format!("{trimmed_base}{path_and_query}")
    } else {
        format!("{trimmed_base}/{path_and_query}")
    }
}

// Normalize REST JSON payloads into a vector of row objects.
fn normalize_rest_payload(payload: Value, limit: Option<u64>) -> Result<Vec<Value>, AdapterError> {
    if let Some(rows_array) = payload.as_array() {
        return Ok(apply_limit(rows_array.clone(), limit));
    }

    if let Some(count_value) = payload.get("count").and_then(|value| value.as_u64()) {
        return Ok(vec![json!({ "count": count_value })]);
    }

    if let Some(data_array) = payload.get("data").and_then(|value| value.as_array()) {
        return Ok(apply_limit(data_array.clone(), limit));
    }

    if let Some(items_array) = payload.get("items").and_then(|value| value.as_array()) {
        return Ok(apply_limit(items_array.clone(), limit));
    }

    if payload.is_object() {
        return Ok(vec![payload]);
    }

    Err(AdapterError::Message(
        "rest response must be a JSON object or array".into(),
    ))
}

fn apply_limit(mut rows: Vec<Value>, limit: Option<u64>) -> Vec<Value> {
    if let Some(limit_value) = limit {
        rows.truncate(limit_value as usize);
    }
    rows
}

fn extract_rest_count(rows: &[Value]) -> u64 {
    if let Some(first_row) = rows.first() {
        if let Some(count_value) = first_row.get("count").and_then(|value| value.as_u64()) {
            return count_value;
        }
    }
    rows.len() as u64
}

// Build a simple REST resource catalog for MCP introspection.
pub fn introspect_rest_schema(base_url: &str, resource_paths: Option<&str>) -> RestSchemaCatalog {
    let resources = if let Some(raw_paths) = resource_paths {
        raw_paths
            .split(',')
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect()
    } else {
        vec!["/".into()]
    };

    RestSchemaCatalog {
        adapter: "rest".into(),
        base_url: base_url.to_string(),
        resources,
    }
}
