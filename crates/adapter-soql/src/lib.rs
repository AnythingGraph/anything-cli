use adapter_core::{AdapterError, DataAdapter, ExecContext, ExecutionState};
use async_trait::async_trait;
use binding_spec::PlaybookBinding;
use plan_ir::{EntityRef, PlanStep, StepResult};
use reqwest::Client;
use serde_json::Value;

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
                let lookup_key = if by_field == "full_name" {
                    "by_name"
                } else {
                    "by_identifier"
                };
                let query_template = entity_binding.lookup.get(lookup_key).ok_or_else(|| {
                    AdapterError::MissingOperation(format!("{entity}.{lookup_key}"))
                })?;
                let soql = query_template.replace(":name", &escape_soql_literal(by_value));
                let records = run_soql_query(context, &self.http_client, &soql).await?;
                let first_record = records.first().ok_or_else(|| {
                    AdapterError::Message(format!("no records for entity resolve: {entity}"))
                })?;
                let id_value = first_record
                    .get(&entity_binding.id_field)
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
                        display_value: first_record
                            .get(by_field)
                            .and_then(|value| value.as_str())
                            .map(|value| value.to_string()),
                    }),
                    count: None,
                    rows: Some(records),
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
                let records = run_soql_query(context, &self.http_client, &soql).await?;
                let count_value = records
                    .first()
                    .and_then(|row| row.get("expr0"))
                    .and_then(|value| value.as_u64())
                    .or_else(|| {
                        records.first().and_then(|row| {
                            row.as_object().and_then(|object| {
                                object.values().next().and_then(|value| value.as_u64())
                            })
                        })
                    })
                    .unwrap_or(0);

                Ok(StepResult {
                    step_index,
                    op: "count_for_subject".into(),
                    entity_ref: None,
                    count: Some(count_value),
                    rows: Some(records),
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
                let records = run_soql_query(context, &self.http_client, &soql).await?;

                Ok(StepResult {
                    step_index,
                    op: "list_for_subject".into(),
                    entity_ref: None,
                    count: None,
                    rows: Some(records),
                    source_query: Some(soql),
                    adapter: Some(self.adapter_type().to_string()),
                })
            }
        }
    }
}

// Execute one SOQL query via Salesforce REST API.
async fn run_soql_query(
    context: &ExecContext,
    http_client: &Client,
    soql: &str,
) -> Result<Vec<Value>, AdapterError> {
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

    Ok(records)
}

// Escape a string literal for SOQL.
fn escape_soql_literal(raw_value: &str) -> String {
    format!("'{}'", raw_value.replace('\'', "\\'"))
}
