use std::collections::HashMap;

use serde_json::Value;

use crate::types::RowKey;

/// Snapshot of one row in the graph used for field comparisons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowSnapshot {
    pub key: RowKey,
    pub values: HashMap<String, Value>,
}

/// One instance-level relationship link between two rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationshipLink {
    pub relationship_name: String,
    pub subject_entity_name: String,
    pub subject_row_id: String,
    pub object_entity_name: String,
    pub object_row_id: String,
}

/// Read-only graph view required to evaluate relationship access rules.
pub trait RebacGraphView {
    /// Return all row keys for an entity type.
    fn list_row_keys(&self, entity_name: &str) -> Vec<RowKey>;

    /// Load field values for one row.
    fn row_snapshot(&self, row_key: &RowKey) -> Option<RowSnapshot>;

    /// Follow a named relationship from subject side to object rows.
    fn follow_forward(
        &self,
        relationship_name: &str,
        from_entity_name: &str,
        from_row_id: &str,
        to_entity_name: &str,
    ) -> Vec<String>;

    /// Follow a named relationship from object side back to subject rows.
    fn follow_reverse(
        &self,
        relationship_name: &str,
        from_entity_name: &str,
        from_row_id: &str,
        to_entity_name: &str,
    ) -> Vec<String>;
}

/// In-memory graph for evaluation and tests.
#[derive(Debug, Default, Clone)]
pub struct MemoryGraph {
    rows: HashMap<(String, String), HashMap<String, Value>>,
    links: Vec<RelationshipLink>,
}

impl MemoryGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert_row(
        mut self,
        entity_name: impl Into<String>,
        row_id: impl Into<String>,
        values: HashMap<String, Value>,
    ) -> Self {
        let entity = entity_name.into();
        let id = row_id.into();
        self.rows.insert((entity, id), values);
        self
    }

    pub fn add_link(mut self, link: RelationshipLink) -> Self {
        self.links.push(link);
        self
    }
}

impl RebacGraphView for MemoryGraph {
    fn list_row_keys(&self, entity_name: &str) -> Vec<RowKey> {
        let mut keys: Vec<RowKey> = self
            .rows
            .keys()
            .filter(|(entity, _)| entity == entity_name)
            .map(|(entity, row_id)| RowKey {
                entity_name: entity.clone(),
                row_id: row_id.clone(),
            })
            .collect();
        keys.sort_by(|left, right| left.row_id.cmp(&right.row_id));
        keys
    }

    fn row_snapshot(&self, row_key: &RowKey) -> Option<RowSnapshot> {
        self.rows
            .get(&(row_key.entity_name.clone(), row_key.row_id.clone()))
            .map(|values| RowSnapshot {
                key: row_key.clone(),
                values: values.clone(),
            })
    }

    fn follow_forward(
        &self,
        relationship_name: &str,
        from_entity_name: &str,
        from_row_id: &str,
        to_entity_name: &str,
    ) -> Vec<String> {
        let mut target_row_ids = Vec::new();
        for link in &self.links {
            if link.relationship_name != relationship_name {
                continue;
            }
            if link.subject_entity_name == from_entity_name
                && link.subject_row_id == from_row_id
                && link.object_entity_name == to_entity_name
            {
                target_row_ids.push(link.object_row_id.clone());
            }
        }
        target_row_ids.sort();
        target_row_ids.dedup();
        target_row_ids
    }

    fn follow_reverse(
        &self,
        relationship_name: &str,
        from_entity_name: &str,
        from_row_id: &str,
        to_entity_name: &str,
    ) -> Vec<String> {
        let mut target_row_ids = Vec::new();
        for link in &self.links {
            if link.relationship_name != relationship_name {
                continue;
            }
            if link.object_entity_name == from_entity_name
                && link.object_row_id == from_row_id
                && link.subject_entity_name == to_entity_name
            {
                target_row_ids.push(link.subject_row_id.clone());
            }
        }
        target_row_ids.sort();
        target_row_ids.dedup();
        target_row_ids
    }
}

/// Read a scalar field from a row as a normalized string for comparisons.
pub fn field_value_as_string(values: &HashMap<String, Value>, field_name: &str) -> Option<String> {
    let field_value = values.get(field_name)?;
    scalar_to_string(field_value)
}

fn scalar_to_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::Bool(boolean) => Some(boolean.to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Array(items) => items.first().and_then(scalar_to_string),
        Value::Object(_) => None,
    }
}

/// Compare two field values using case-insensitive trimmed string equality.
pub fn field_values_equal(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}
