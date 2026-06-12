use std::collections::HashMap;

use playbook_spec::PlaybookDefinition;

use crate::{EntityBinding, PlaybookBinding, RelationshipBinding};

// Fill missing lookup SQL and relationship operations from declarative metadata.
pub fn compile_binding_queries(binding: &mut PlaybookBinding) {
    for entity_binding in binding.entities.values_mut() {
        compile_entity_lookups(entity_binding);
    }

    let relationship_names: Vec<String> = binding.relationships.keys().cloned().collect();
    for relationship_name in relationship_names {
        if let Some(relationship_binding) = binding.relationships.get_mut(&relationship_name) {
            compile_relationship_operations(&binding.entities, relationship_binding);
        }
    }
}

// Generate entity lookup queries when only table/column metadata is present.
fn compile_entity_lookups(entity_binding: &mut EntityBinding) {
    let table_name = match entity_binding.from.as_ref() {
        Some(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => return,
    };

    let id_column = entity_binding.id_field.clone();
    let select_columns = build_select_column_list(entity_binding, &id_column);

    if !entity_binding.lookup.contains_key("by_name") {
        if let Some(name_column) = resolve_name_column(entity_binding) {
            entity_binding.lookup.insert(
                "by_name".into(),
                format!(
                    "SELECT {select_columns} FROM {table_name} WHERE {name_column} ILIKE :name LIMIT 1"
                ),
            );
        }
    }

    if !entity_binding.lookup.contains_key("by_identifier") {
        entity_binding.lookup.insert(
            "by_identifier".into(),
            format!(
                "SELECT {select_columns} FROM {table_name} WHERE {id_column} = :identifier LIMIT 1"
            ),
        );
    }
}

// Generate count/list SQL for relationships when subject_link_column is set.
fn compile_relationship_operations(
    entity_bindings: &HashMap<String, EntityBinding>,
    relationship_binding: &mut RelationshipBinding,
) {
    let subject_link_column = match relationship_binding.subject_link_column.as_ref() {
        Some(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => return,
    };

    let object_entity_name = match relationship_binding.join.as_ref() {
        Some(join) => join.to_entity.clone(),
        None => return,
    };

    let object_binding = match entity_bindings.get(&object_entity_name) {
        Some(value) => value,
        None => return,
    };

    let object_table = match object_binding.from.as_ref() {
        Some(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => return,
    };

    let list_columns = if object_binding.fields.is_empty() {
        "*".to_string()
    } else {
        object_binding
            .fields
            .values()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    };

    if !relationship_binding
        .operations
        .contains_key("count_for_subject")
    {
        relationship_binding.operations.insert(
            "count_for_subject".into(),
            format!(
                "SELECT COUNT(*)::bigint AS count FROM {object_table} WHERE {subject_link_column} = :subject_id"
            ),
        );
    }

    if !relationship_binding
        .operations
        .contains_key("list_for_subject")
    {
        relationship_binding.operations.insert(
            "list_for_subject".into(),
            format!(
                "SELECT {list_columns} FROM {object_table} WHERE {subject_link_column} = :subject_id LIMIT :limit"
            ),
        );
    }
}

// Build SELECT column list from entity field mappings.
fn build_select_column_list(entity_binding: &EntityBinding, id_column: &str) -> String {
    if entity_binding.fields.is_empty() {
        return id_column.to_string();
    }

    let mut columns: Vec<String> = Vec::new();
    for column_name in entity_binding.fields.values() {
        if !columns.iter().any(|existing| existing == column_name) {
            columns.push(column_name.clone());
        }
    }
    if !columns.iter().any(|column| column == id_column) {
        columns.insert(0, id_column.to_string());
    }
    columns.join(", ")
}

// Pick a display-name column for by_name lookup.
fn resolve_name_column(entity_binding: &EntityBinding) -> Option<String> {
    let preferred_keys = ["full_name", "name", "legal_name", "display_name", "title"];
    for preferred_key in preferred_keys {
        if let Some(column_name) = entity_binding.fields.get(preferred_key) {
            return Some(column_name.clone());
        }
    }

    entity_binding
        .fields
        .iter()
        .find(|(field_name, _)| field_name.as_str() != entity_binding.id_field.as_str())
        .map(|(_, column_name)| column_name.clone())
}

// Validate binding entity/relationship names against a playbook definition.
pub fn validate_binding_for_playbook(
    playbook: &PlaybookDefinition,
    binding: &PlaybookBinding,
) -> BindingValidationReport {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    for entity in &playbook.entities {
        if !binding.entities.contains_key(&entity.name) {
            warnings.push(format!(
                "playbook entity '{}' has no binding mapping",
                entity.name
            ));
        }
    }

    for (entity_name, _) in &binding.entities {
        if !playbook.entities.iter().any(|entity| &entity.name == entity_name) {
            warnings.push(format!(
                "binding entity '{entity_name}' is not defined in playbook '{}'",
                playbook.id
            ));
        }
    }

    for relationship in &playbook.entity_relationships {
        if !binding.relationships.contains_key(&relationship.relationship_name) {
            warnings.push(format!(
                "playbook relationship '{}' has no binding operations",
                relationship.relationship_name
            ));
            continue;
        }

        let relationship_binding = &binding.relationships[&relationship.relationship_name];
        if !relationship_binding
            .operations
            .contains_key("count_for_subject")
            && !relationship_binding
                .operations
                .contains_key("list_for_subject")
        {
            errors.push(format!(
                "relationship '{}' is missing count_for_subject or list_for_subject operations",
                relationship.relationship_name
            ));
        }
    }

    for entity_binding in binding.entities.values() {
        if entity_binding.lookup.is_empty() {
            errors.push(format!(
                "entity binding for id_field '{}' has no lookup queries (add lookup or from/fields metadata)",
                entity_binding.id_field
            ));
        }
    }

    if binding.adapter.trim().is_empty() {
        errors.push("binding adapter is required".into());
    }

    BindingValidationReport {
        valid: errors.is_empty(),
        errors,
        warnings,
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BindingValidationReport {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

// Build playbook-scoped binding file stem, e.g. simple-crm-access.postgres.
pub fn playbook_binding_stem(playbook_id: &str, adapter_suffix: &str) -> String {
    format!("{playbook_id}.{adapter_suffix}")
}

// Suggest table matches for playbook entities from source table names.
pub fn suggest_entity_table_mappings(
    playbook: &PlaybookDefinition,
    table_names: &[String],
) -> HashMap<String, String> {
    let mut suggestions = HashMap::new();

    for entity in &playbook.entities {
        if let Some(table_name) = find_best_table_match(&entity.name, table_names) {
            suggestions.insert(entity.name.clone(), table_name);
        }
    }

    suggestions
}

// Score entity name against available tables using simple heuristics.
fn find_best_table_match(entity_name: &str, table_names: &[String]) -> Option<String> {
    let normalized_entity = normalize_identifier(entity_name);

    for table_name in table_names {
        if normalize_identifier(table_name) == normalized_entity {
            return Some(table_name.clone());
        }
    }

    for table_name in table_names {
        let normalized_table = normalize_identifier(table_name);
        if normalized_entity.ends_with(&normalized_table) || normalized_table.ends_with(&normalized_entity)
        {
            return Some(table_name.clone());
        }
    }

    None
}

// Normalize names for fuzzy table matching.
fn normalize_identifier(raw_value: &str) -> String {
    raw_value
        .trim()
        .to_ascii_lowercase()
        .replace("crm_", "")
        .replace("fin_", "")
        .replace('-', "_")
}
