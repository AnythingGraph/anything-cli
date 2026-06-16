use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Default row cap for list_entity when limit is omitted.
pub const DEFAULT_LIST_ENTITY_LIMIT: u32 = 1000;

/// Default row cap for sample_entity when limit is omitted.
pub const DEFAULT_SAMPLE_ENTITY_LIMIT: u32 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRequest {
    pub playbook_id: String,
    #[serde(default)]
    pub subject_id: Option<String>,
    #[serde(default)]
    pub binding_name: Option<String>,
    #[serde(default)]
    pub resolve: Option<ResolveEntityRequest>,
    #[serde(default)]
    pub list_entity: Option<ListEntityRequest>,
    #[serde(default)]
    pub sample_entity: Option<SampleEntityRequest>,
    #[serde(default)]
    pub count: Option<CountRelationshipRequest>,
    #[serde(default)]
    pub list: Option<ListRelationshipRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveEntityRequest {
    pub entity: String,
    #[serde(default)]
    pub by_name: Option<String>,
    #[serde(default)]
    pub by_identifier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListEntityRequest {
    pub entity: String,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleEntityRequest {
    pub entity: String,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountRelationshipRequest {
    pub relationship: String,
    #[serde(default)]
    pub object_entity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListRelationshipRequest {
    pub relationship: String,
    #[serde(default)]
    pub object_entity: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub playbook_id: String,
    #[serde(default)]
    pub subject_id: Option<String>,
    #[serde(default)]
    pub binding_name: Option<String>,
    pub steps: Vec<PlanStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PlanStep {
    ResolveEntity {
        entity: String,
        by_field: String,
        by_value: String,
    },
    ListEntity {
        entity: String,
        limit: u32,
        #[serde(default)]
        sample: bool,
    },
    CountForSubject {
        relationship: String,
        object_entity: Option<String>,
    },
    ListForSubject {
        relationship: String,
        object_entity: Option<String>,
        limit: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRef {
    pub entity: String,
    pub id_field: String,
    pub id_value: String,
    #[serde(default)]
    pub display_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_index: usize,
    pub op: String,
    #[serde(default)]
    pub entity_ref: Option<EntityRef>,
    #[serde(default)]
    pub count: Option<u64>,
    #[serde(default)]
    pub rows: Option<Vec<Value>>,
    #[serde(default)]
    pub source_query: Option<String>,
    #[serde(default)]
    pub adapter: Option<String>,
}
