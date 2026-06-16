use std::collections::HashMap;

use adapter_core::{
    AdapterError, DataAdapter, ExecContext, ExecutionState, map_row_to_playbook_fields,
    row_map_to_json,
};
use async_trait::async_trait;
use binding_spec::PlaybookBinding;
use mongodb::bson::{doc, Document};
use mongodb::options::ClientOptions;
use mongodb::Client;
use plan_ir::{EntityRef, PlanStep, StepResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

mod authoring;

pub use authoring::authoring_guide;

pub struct MongoDbAdapter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MongoDbSchemaCatalog {
    pub adapter: String,
    pub database: String,
    pub collections: Vec<String>,
}

#[derive(Debug, Clone)]
struct MongoOperation {
    operation_type: String,
    collection_name: String,
    filter_json: String,
    limit: Option<u64>,
}

#[async_trait]
impl DataAdapter for MongoDbAdapter {
    fn adapter_type(&self) -> &'static str {
        "mongodb"
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
                let operation_template = entity_binding.lookup.get(lookup_key).ok_or_else(|| {
                    AdapterError::MissingOperation(format!("{entity}.{lookup_key}"))
                })?;
                let operation = parse_mongo_operation(operation_template)?;
                let filter = build_mongo_filter(&operation.filter_json, by_value, None)?;
                let rows = run_mongo_find(context, &operation.collection_name, filter, Some(1)).await?;
                let first_row = rows.first().ok_or_else(|| {
                    AdapterError::Message(format!("no mongodb rows for entity resolve: {entity}"))
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
                    source_query: Some(operation_template.clone()),
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
                let operation = parse_mongo_operation(operation_template)?;
                let filter = build_mongo_filter(&operation.filter_json, &subject.id_value, None)?;
                let count_value = run_mongo_count(context, &operation.collection_name, filter).await?;

                Ok(StepResult {
                    step_index,
                    op: "count_for_subject".into(),
                    entity_ref: None,
                    count: Some(count_value),
                    rows: None,
                    source_query: Some(operation_template.clone()),
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
                let operation = parse_mongo_operation(operation_template)?;
                let filter = build_mongo_filter(
                    &operation.filter_json,
                    &subject.id_value,
                    Some(u64::from(*limit)),
                )?;
                let rows = run_mongo_find(
                    context,
                    &operation.collection_name,
                    filter,
                    Some(u64::from(*limit)),
                )
                .await?;

                Ok(StepResult {
                    step_index,
                    op: "list_for_subject".into(),
                    entity_ref: None,
                    count: None,
                    rows: Some(rows),
                    source_query: Some(operation_template.clone()),
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
                let operation_template = entity_binding
                    .operations
                    .get("list_entity")
                    .ok_or_else(|| {
                        AdapterError::MissingOperation(format!("{entity}.operations.list_entity"))
                    })?;
                let operation = parse_mongo_operation(operation_template)?;
                let filter = build_mongo_filter(&operation.filter_json, "", Some(u64::from(*limit)))?;
                let physical_rows = run_mongo_find(
                    context,
                    &operation.collection_name,
                    filter,
                    Some(u64::from(*limit)),
                )
                .await?;

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
                    source_query: Some(operation_template.clone()),
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
        let operation = parse_mongo_operation(operation_template)?;
        let filter = build_mongo_filter(&operation.filter_json, "", None)?;
        let physical_rows = run_mongo_find(context, &operation.collection_name, filter, None).await?;

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

// Parse compiled MongoDB operation strings such as find:users:{"name":":name"}:limit=10.
fn parse_mongo_operation(raw_operation: &str) -> Result<MongoOperation, AdapterError> {
    let mut parts: Vec<&str> = raw_operation.splitn(4, ':').collect();
    if parts.len() < 3 {
        return Err(AdapterError::Message(format!(
            "invalid mongodb operation: {raw_operation}"
        )));
    }

    let operation_type = parts.remove(0).to_string();
    let collection_name = parts.remove(0).to_string();
    let mut filter_json = parts.remove(0).to_string();
    let mut limit = None;

    if let Some(extra) = parts.first() {
        if let Some(limit_text) = extra.strip_prefix("limit=") {
            limit = limit_text.parse::<u64>().ok();
        } else if !extra.trim().is_empty() {
            filter_json = format!("{filter_json}:{extra}");
        }
    }

    Ok(MongoOperation {
        operation_type,
        collection_name,
        filter_json,
        limit,
    })
}

// Replace :name, :identifier, and :subject_id placeholders then parse JSON filter.
fn build_mongo_filter(
    filter_template: &str,
    primary_value: &str,
    limit: Option<u64>,
) -> Result<Document, AdapterError> {
    let escaped = primary_value.replace('\\', "\\\\").replace('"', "\\\"");
    let mut filter_text = filter_template.replace(":identifier", &escaped);
    filter_text = filter_text.replace(":subject_id", &escaped);
    filter_text = filter_text.replace(":name", &escaped);
    if let Some(limit_value) = limit {
        filter_text = filter_text.replace(":limit", &limit_value.to_string());
    }

    if filter_text.trim().is_empty() || filter_text.trim() == "{}" {
        return Ok(doc! {});
    }

    let parsed: Document = serde_json::from_str(&filter_text).map_err(|error| {
        AdapterError::Message(format!("invalid mongodb filter json: {error}"))
    })?;
    Ok(parsed)
}

// Connect to MongoDB using profile dsn and optional database name.
async fn connect_mongo_client(context: &ExecContext) -> Result<(Client, String), AdapterError> {
    let dsn = context
        .connection
        .dsn
        .as_ref()
        .ok_or_else(|| AdapterError::Message("mongodb adapter requires dsn in profile".into()))?;

    let database_name = context
        .connection
        .database
        .clone()
        .or_else(|| std::env::var("AG_MONGODB_DATABASE").ok())
        .unwrap_or_else(|| "anythinggraph".to_string());

    let client_options = ClientOptions::parse(dsn)
        .await
        .map_err(|error| AdapterError::Message(format!("mongodb parse dsn failed: {error}")))?;
    let client = Client::with_options(client_options).map_err(|error| {
        AdapterError::Message(format!("mongodb client init failed: {error}"))
    })?;

    Ok((client, database_name))
}

// Run a find query and map BSON documents to JSON rows with playbook field names.
async fn run_mongo_find(
    context: &ExecContext,
    collection_name: &str,
    filter: Document,
    limit: Option<u64>,
) -> Result<Vec<Value>, AdapterError> {
    let (client, database_name) = connect_mongo_client(context).await?;
    let collection = client.database(&database_name).collection::<Document>(collection_name);

    let mut find_action = collection.find(filter);
    if let Some(limit_value) = limit {
        find_action = find_action.limit(limit_value as i64);
    }

    let mut cursor = find_action
        .await
        .map_err(|error| AdapterError::Message(format!("mongodb find failed: {error}")))?;

    let mut rows = Vec::new();
    while cursor
        .advance()
        .await
        .map_err(|error| AdapterError::Message(format!("mongodb cursor failed: {error}")))?
    {
        let document = cursor.deserialize_current().map_err(|error| {
            AdapterError::Message(format!("mongodb deserialize failed: {error}"))
        })?;
        rows.push(bson_document_to_json(&document));
    }
    Ok(rows)
}

// Run a count query against a MongoDB collection.
async fn run_mongo_count(
    context: &ExecContext,
    collection_name: &str,
    filter: Document,
) -> Result<u64, AdapterError> {
    let (client, database_name) = connect_mongo_client(context).await?;
    let collection = client.database(&database_name).collection::<Document>(collection_name);
    let count_value = collection
        .count_documents(filter)
        .await
        .map_err(|error| AdapterError::Message(format!("mongodb count failed: {error}")))?;
    Ok(count_value)
}

// Convert BSON document to JSON value for downstream mapping.
fn bson_document_to_json(document: &Document) -> Value {
    serde_json::to_value(document).unwrap_or_else(|_| json!({}))
}

// List collection names for MCP introspection.
pub async fn introspect_mongodb_schema(
    dsn: &str,
    database_name: Option<&str>,
) -> Result<MongoDbSchemaCatalog, AdapterError> {
    let database_name = database_name
        .map(|value| value.to_string())
        .or_else(|| std::env::var("AG_MONGODB_DATABASE").ok())
        .unwrap_or_else(|| "anythinggraph".to_string());

    let client_options = ClientOptions::parse(dsn)
        .await
        .map_err(|error| AdapterError::Message(format!("mongodb parse dsn failed: {error}")))?;
    let client = Client::with_options(client_options).map_err(|error| {
        AdapterError::Message(format!("mongodb client init failed: {error}"))
    })?;

    let collection_names = client
        .database(&database_name)
        .list_collection_names()
        .await
        .map_err(|error| AdapterError::Message(format!("mongodb list collections failed: {error}")))?;

    Ok(MongoDbSchemaCatalog {
        adapter: "mongodb".into(),
        database: database_name,
        collections: collection_names,
    })
}

// Return up to `limit` raw documents from one collection (read-only discovery; no playbook).
pub async fn sample_mongodb_collection(
    dsn: &str,
    database_name: Option<&str>,
    collection_name: &str,
    limit: u32,
) -> Result<(String, Vec<Value>), AdapterError> {
    if collection_name.trim().is_empty() {
        return Err(AdapterError::Message("collection name is required".into()));
    }

    let database_name = database_name
        .map(|value| value.to_string())
        .or_else(|| std::env::var("AG_MONGODB_DATABASE").ok())
        .unwrap_or_else(|| "anythinggraph".to_string());

    let source_query = format!("find:{collection_name}:{{}}:limit={limit}");
    let (client, _) = connect_mongo_client_for_dsn(dsn, &database_name).await?;
    let collection = client
        .database(&database_name)
        .collection::<Document>(collection_name);

    let mut cursor = collection
        .find(doc! {})
        .limit(limit as i64)
        .await
        .map_err(|error| AdapterError::Message(format!("mongodb find failed: {error}")))?;

    let mut rows = Vec::new();
    while cursor
        .advance()
        .await
        .map_err(|error| AdapterError::Message(format!("mongodb cursor failed: {error}")))?
    {
        let document = cursor.deserialize_current().map_err(|error| {
            AdapterError::Message(format!("mongodb deserialize failed: {error}"))
        })?;
        rows.push(bson_document_to_json(&document));
    }

    Ok((source_query, rows))
}

// Connect to MongoDB for source-level sampling helpers.
async fn connect_mongo_client_for_dsn(
    dsn: &str,
    database_name: &str,
) -> Result<(Client, String), AdapterError> {
    let _ = database_name;
    let client_options = ClientOptions::parse(dsn)
        .await
        .map_err(|error| AdapterError::Message(format!("mongodb parse dsn failed: {error}")))?;
    let client = Client::with_options(client_options).map_err(|error| {
        AdapterError::Message(format!("mongodb client init failed: {error}"))
    })?;
    Ok((client, database_name.to_string()))
}