use async_trait::async_trait;
use binding_spec::{EntityBinding, PlaybookBinding, SourceProfile};
use plan_ir::{EntityRef, PlanStep, StepResult};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("adapter error: {0}")]
    Message(String),
    #[error("binding missing for entity: {0}")]
    MissingEntityBinding(String),
    #[error("operation missing: {0}")]
    MissingOperation(String),
}

#[derive(Debug, Clone)]
pub struct ExecContext {
    pub adapter_type: String,
    pub source_id: Option<String>,
    pub profile: SourceProfile,
    pub connection: binding_spec::SourceConnection,
}

#[derive(Debug, Clone, Default)]
pub struct ExecutionState {
    pub current_subject: Option<EntityRef>,
}

// Federated data source adapter (SQL, SOQL, REST, ...).
#[async_trait]
pub trait DataAdapter: Send + Sync {
    fn adapter_type(&self) -> &'static str;

    async fn execute_step(
        &self,
        step_index: usize,
        step: &PlanStep,
        binding: &PlaybookBinding,
        context: &ExecContext,
        state: &ExecutionState,
    ) -> Result<StepResult, AdapterError>;

    /// Load all rows for one entity (used by ReBAC graph materialization).
    async fn load_entity_rows(
        &self,
        entity_name: &str,
        binding: &PlaybookBinding,
        context: &ExecContext,
    ) -> Result<Vec<Value>, AdapterError>;
}

pub struct AdapterRegistry {
    adapters: HashMap<String, Arc<dyn DataAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
        }
    }

    pub fn register(&mut self, adapter: Arc<dyn DataAdapter>) {
        self.adapters
            .insert(adapter.adapter_type().to_string(), adapter);
    }

    pub fn get(&self, adapter_type: &str) -> Option<Arc<dyn DataAdapter>> {
        self.adapters.get(adapter_type).cloned()
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// Build execution context for one binding + profile.
pub fn build_exec_context(
    binding: &PlaybookBinding,
    profile: &SourceProfile,
) -> Result<ExecContext, AdapterError> {
    let adapter_type = binding_spec::resolve_adapter_type(binding, profile)
        .map_err(|error| AdapterError::Message(error.to_string()))?;

    let source_id = binding.source_id.clone();
    let connection = if let Some(source_id_value) = source_id.as_ref() {
        profile
            .sources
            .get(source_id_value)
            .cloned()
            .ok_or_else(|| {
                AdapterError::Message(format!("profile missing source: {source_id_value}"))
            })?
    } else {
        binding_spec::SourceConnection {
            adapter: adapter_type.clone(),
            dsn: std::env::var("AG_SQL_DSN").ok(),
            instance_url: std::env::var("AG_SF_INSTANCE_URL").ok(),
            auth: std::env::var("AG_SF_ACCESS_TOKEN").ok(),
            file_path: std::env::var("AG_PAYROLL_CSV_PATH").ok(),
            base_url: std::env::var("AG_REST_BASE_URL").ok(),
            database: std::env::var("AG_MONGODB_DATABASE").ok(),
        }
    };

    Ok(ExecContext {
        adapter_type,
        source_id,
        profile: profile.clone(),
        connection,
    })
}

// Convert generic row map into JSON value.
pub fn row_map_to_json(row: HashMap<String, Value>) -> Value {
    Value::Object(row.into_iter().collect())
}

// Map a source row (physical column names) to playbook field names.
pub fn map_row_to_playbook_fields(
    row: HashMap<String, Value>,
    entity_binding: &EntityBinding,
) -> HashMap<String, Value> {
    let mut mapped = HashMap::new();

    for (playbook_field, physical_column) in &entity_binding.fields {
        if let Some(value) = row.get(physical_column) {
            mapped.insert(playbook_field.clone(), value.clone());
        }
    }

    if let Some(value) = row.get(&entity_binding.id_field) {
        mapped.insert(entity_binding.id_field.clone(), value.clone());
    }

    for (column_name, value) in row {
        if !mapped.contains_key(&column_name) {
            mapped.insert(column_name, value);
        }
    }

    mapped
}
