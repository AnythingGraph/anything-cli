use std::collections::{HashMap, HashSet};

use binding_spec::{EntityBinding, PlaybookBinding, RelationshipBinding};
use playbook_spec::PlaybookDefinition;
use serde_json::Value;

use crate::graph::{field_values_equal, MemoryGraph, RelationshipLink};

/// Rows keyed by playbook entity name (field names are playbook field names).
#[derive(Debug, Clone, Default)]
pub struct GraphSnapshot {
    pub rows_by_entity: HashMap<String, Vec<HashMap<String, Value>>>,
    pub entity_id_fields: HashMap<String, String>,
}

/// Build an in-memory ReBAC graph from playbook metadata, bindings, and loaded rows.
pub fn build_memory_graph(
    playbook: &PlaybookDefinition,
    bindings: &[&PlaybookBinding],
    snapshot: &GraphSnapshot,
) -> MemoryGraph {
    let mut graph = MemoryGraph::new();

    for (entity_name, rows) in &snapshot.rows_by_entity {
        let id_field = snapshot
            .entity_id_fields
            .get(entity_name)
            .cloned()
            .unwrap_or_else(|| "id".to_string());

        for row in rows {
            let row_id = row
                .get(&id_field)
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());

            if let Some(row_id_value) = row_id {
                graph = graph.upsert_row(entity_name.clone(), row_id_value, row.clone());
            }
        }
    }

    for relationship in &playbook.entity_relationships {
        let relationship_binding =
            find_relationship_binding(bindings, &relationship.relationship_name);
        let link_field_on_object =
            resolve_object_link_playbook_field(relationship_binding, bindings, relationship);

        let object_rows = snapshot
            .rows_by_entity
            .get(&relationship.object_entity_name)
            .cloned()
            .unwrap_or_default();
        let subject_rows = snapshot
            .rows_by_entity
            .get(&relationship.subject_entity_name)
            .cloned()
            .unwrap_or_default();

        let subject_id_field = snapshot
            .entity_id_fields
            .get(&relationship.subject_entity_name)
            .cloned()
            .unwrap_or_else(|| "id".to_string());

        if link_field_on_object.is_none() {
            continue;
        }
        let link_field = link_field_on_object.expect("checked above");

        for object_row in &object_rows {
            let link_value = object_row
                .get(&link_field)
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            if link_value.is_none() {
                continue;
            }
            let link_value = link_value.expect("checked above");

            let object_id_field = snapshot
                .entity_id_fields
                .get(&relationship.object_entity_name)
                .cloned()
                .unwrap_or_else(|| "id".to_string());
            let object_row_id = object_row
                .get(&object_id_field)
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            if object_row_id.is_none() {
                continue;
            }
            let object_row_id = object_row_id.expect("checked above");

            for subject_row in &subject_rows {
                let subject_id_value = subject_row
                    .get(&subject_id_field)
                    .and_then(|value| value.as_str())
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                if subject_id_value.is_none() {
                    continue;
                }
                let subject_id_value = subject_id_value.expect("checked above");

                if field_values_equal(&link_value, &subject_id_value) {
                    graph = graph.add_link(RelationshipLink {
                        relationship_name: relationship.relationship_name.clone(),
                        subject_entity_name: relationship.subject_entity_name.clone(),
                        subject_row_id: subject_id_value,
                        object_entity_name: relationship.object_entity_name.clone(),
                        object_row_id: object_row_id.clone(),
                    });
                }
            }
        }
    }

    graph
}

/// Collect entity id fields from bindings for a playbook.
pub fn entity_id_fields_from_bindings(
    playbook: &PlaybookDefinition,
    bindings: &HashMap<String, PlaybookBinding>,
) -> HashMap<String, String> {
    let mut id_fields = HashMap::new();

    for entity in &playbook.entities {
        if let Some(binding) = binding_for_entity(playbook, bindings, &entity.name) {
            if let Some(entity_binding) = binding.entities.get(&entity.name) {
                id_fields.insert(entity.name.clone(), entity_binding.id_field.clone());
            }
        } else if let Some(identifier_field) = entity
            .fields
            .iter()
            .find(|field| field.is_identifier)
            .map(|field| field.field_name.clone())
        {
            id_fields.insert(entity.name.clone(), identifier_field);
        }
    }

    id_fields
}

/// Find the binding that maps a playbook entity (via entity_sources + bindings map).
pub fn binding_for_entity(
    playbook: &PlaybookDefinition,
    bindings: &HashMap<String, PlaybookBinding>,
    entity_name: &str,
) -> Option<PlaybookBinding> {
    let binding_stem = playbook_spec::resolve_binding_name_for_entity(playbook, entity_name);
    binding_stem
        .and_then(|stem| bindings.get(&stem).cloned())
        .or_else(|| {
            bindings
                .values()
                .find(|binding| binding.entities.contains_key(entity_name))
                .cloned()
        })
}

fn find_relationship_binding<'binding>(
    bindings: &'binding [&PlaybookBinding],
    relationship_name: &str,
) -> Option<&'binding RelationshipBinding> {
    for binding in bindings {
        if let Some(relationship_binding) = binding.relationships.get(relationship_name) {
            return Some(relationship_binding);
        }
    }
    None
}

fn resolve_object_link_playbook_field(
    relationship_binding: Option<&RelationshipBinding>,
    bindings: &[&PlaybookBinding],
    relationship: &playbook_spec::PlaybookEntityRelationship,
) -> Option<String> {
    let physical_link_column = relationship_binding
        .and_then(|binding| binding.subject_link_column.as_ref())
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());

    if physical_link_column.is_none() {
        return infer_link_field_from_object_entity(bindings, &relationship.object_entity_name);
    }
    let physical_link_column = physical_link_column.expect("checked above");

    for binding in bindings {
        if let Some(object_binding) = binding.entities.get(&relationship.object_entity_name) {
            if let Some(playbook_field) =
                playbook_field_for_physical_column(object_binding, physical_link_column)
            {
                return Some(playbook_field);
            }
        }
    }

    Some(physical_link_column.to_string())
}

fn playbook_field_for_physical_column(
    entity_binding: &EntityBinding,
    physical_column: &str,
) -> Option<String> {
    for (playbook_field, column_name) in &entity_binding.fields {
        if column_name.eq_ignore_ascii_case(physical_column) {
            return Some(playbook_field.clone());
        }
    }
    if entity_binding.id_field.eq_ignore_ascii_case(physical_column) {
        return Some(entity_binding.id_field.clone());
    }
    None
}

fn infer_link_field_from_object_entity(
    bindings: &[&PlaybookBinding],
    object_entity_name: &str,
) -> Option<String> {
    for binding in bindings {
        if let Some(object_binding) = binding.entities.get(object_entity_name) {
            for preferred_field in ["user_id", "owner_user_id", "subject_id", "owner_id"] {
                if object_binding.fields.contains_key(preferred_field)
                    || object_binding.id_field == preferred_field
                {
                    return Some(preferred_field.to_string());
                }
            }
        }
    }
    None
}

/// Extra physical columns needed on object entities for link materialization.
pub fn link_columns_for_entity(
    playbook: &PlaybookDefinition,
    bindings: &HashMap<String, PlaybookBinding>,
    entity_name: &str,
) -> HashSet<String> {
    let mut columns = HashSet::new();

    for relationship in &playbook.entity_relationships {
        if relationship.object_entity_name != entity_name {
            continue;
        }
        let binding_list: Vec<&PlaybookBinding> = bindings.values().collect();
        let relationship_binding =
            find_relationship_binding(&binding_list, &relationship.relationship_name);
        if let Some(link_column) = relationship_binding
            .and_then(|binding| binding.subject_link_column.as_ref())
        {
            columns.insert(link_column.trim().to_string());
        }
    }

    columns
}
