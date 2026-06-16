use std::collections::HashMap;

use playbook_spec::PlaybookDefinition;

use crate::{EntityBinding, PlaybookBinding, RelationshipBinding};

// Fill missing lookup SQL and relationship operations from declarative metadata.
pub fn compile_binding_queries(binding: &mut PlaybookBinding) {
    let adapter_type = binding.adapter.trim().to_ascii_lowercase();

    if adapter_type == "mongodb" {
        compile_mongodb_binding_queries(binding);
        return;
    }

    if adapter_type == "rest" {
        compile_rest_binding_queries(binding);
        return;
    }

    compile_entity_list_all_queries(binding, &adapter_type);

    for entity_binding in binding.entities.values_mut() {
        compile_entity_lookups(entity_binding, &adapter_type);
    }

    let relationship_names: Vec<String> = binding.relationships.keys().cloned().collect();
    for relationship_name in relationship_names {
        if let Some(relationship_binding) = binding.relationships.get_mut(&relationship_name) {
            compile_relationship_operations(&binding.entities, relationship_binding, &adapter_type);
        }
    }
}

// Generate list_all queries for entities (used by ReBAC graph materialization).
fn compile_entity_list_all_queries(binding: &mut PlaybookBinding, adapter_type: &str) {
    let object_link_columns: HashMap<String, Vec<String>> =
        collect_object_link_columns(binding);

    for (entity_name, entity_binding) in binding.entities.iter_mut() {
        let table_name = match entity_binding.from.as_ref() {
            Some(value) if !value.trim().is_empty() => value.trim().to_string(),
            _ => continue,
        };

        let id_column = entity_binding.id_field.clone();
        let mut select_columns = build_select_column_list(entity_binding, &id_column);

        if let Some(extra_columns) = object_link_columns.get(entity_name) {
            for extra_column in extra_columns {
                if !select_columns.contains(extra_column) {
                    if select_columns == id_column {
                        select_columns = format!("{select_columns}, {extra_column}");
                    } else {
                        select_columns = format!("{select_columns}, {extra_column}");
                    }
                }
            }
        }

        if !entity_binding.operations.contains_key("list_all") {
            entity_binding.operations.insert(
                "list_all".into(),
                format_list_all_query(adapter_type, &select_columns, &table_name),
            );
        }

        if !entity_binding.operations.contains_key("list_entity") {
            entity_binding.operations.insert(
                "list_entity".into(),
                format_list_entity_query(adapter_type, &select_columns, &table_name),
            );
        }
    }
}

// Build dialect-specific list_entity query text (bounded list for browse/sample).
fn format_list_entity_query(adapter_type: &str, select_columns: &str, table_name: &str) -> String {
    match adapter_type {
        "mssql" => format!("SELECT TOP (:limit) {select_columns} FROM {table_name}"),
        _ => format!("SELECT {select_columns} FROM {table_name} LIMIT :limit"),
    }
}

// Build dialect-specific list_all query text.
fn format_list_all_query(adapter_type: &str, select_columns: &str, table_name: &str) -> String {
    match adapter_type {
        "mssql" => format!("SELECT {select_columns} FROM {table_name}"),
        _ => format!("SELECT {select_columns} FROM {table_name}"),
    }
}

// Map object entities to subject_link_column values for list_all SELECT expansion.
fn collect_object_link_columns(binding: &PlaybookBinding) -> HashMap<String, Vec<String>> {
    let mut columns_by_entity: HashMap<String, Vec<String>> = HashMap::new();

    for relationship_binding in binding.relationships.values() {
        let object_entity_name = match relationship_binding.join.as_ref() {
            Some(join) => join.to_entity.clone(),
            None => continue,
        };
        let link_column = match relationship_binding.subject_link_column.as_ref() {
            Some(value) if !value.trim().is_empty() => value.trim().to_string(),
            _ => continue,
        };

        let entry = columns_by_entity
            .entry(object_entity_name)
            .or_insert_with(Vec::new);
        if !entry.iter().any(|existing| existing == &link_column) {
            entry.push(link_column);
        }
    }

    columns_by_entity
}

// Generate entity lookup queries when only table/column metadata is present.
fn compile_entity_lookups(entity_binding: &mut EntityBinding, adapter_type: &str) {
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
                format_name_lookup(adapter_type, &select_columns, &table_name, &name_column),
            );
        }
    }

    if !entity_binding.lookup.contains_key("by_identifier") {
        entity_binding.lookup.insert(
            "by_identifier".into(),
            format_identifier_lookup(adapter_type, &select_columns, &table_name, &id_column),
        );
    }
}

// Build dialect-specific name lookup query.
fn format_name_lookup(
    adapter_type: &str,
    select_columns: &str,
    table_name: &str,
    name_column: &str,
) -> String {
    match adapter_type {
        "mysql" => format!(
            "SELECT {select_columns} FROM {table_name} WHERE LOWER({name_column}) LIKE CONCAT('%', LOWER(:name), '%') LIMIT 1"
        ),
        "mssql" => format!(
            "SELECT TOP 1 {select_columns} FROM {table_name} WHERE LOWER({name_column}) = LOWER(:name)"
        ),
        "soql" => format!(
            "SELECT {select_columns} FROM {table_name} WHERE {name_column} = :name LIMIT 1"
        ),
        _ => format!(
            "SELECT {select_columns} FROM {table_name} WHERE {name_column} ILIKE '%' || :name || '%' LIMIT 1"
        ),
    }
}

// Build dialect-specific identifier lookup query.
fn format_identifier_lookup(
    adapter_type: &str,
    select_columns: &str,
    table_name: &str,
    id_column: &str,
) -> String {
    match adapter_type {
        "mssql" => format!(
            "SELECT TOP 1 {select_columns} FROM {table_name} WHERE {id_column} = :identifier"
        ),
        "soql" => format!(
            "SELECT {select_columns} FROM {table_name} WHERE {id_column} = :identifier LIMIT 1"
        ),
        _ => format!(
            "SELECT {select_columns} FROM {table_name} WHERE {id_column} = :identifier LIMIT 1"
        ),
    }
}

// Generate count/list SQL for relationships when subject_link_column is set.
fn compile_relationship_operations(
    entity_bindings: &HashMap<String, EntityBinding>,
    relationship_binding: &mut RelationshipBinding,
    adapter_type: &str,
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
            format_count_for_subject(adapter_type, &object_table, &subject_link_column),
        );
    }

    if !relationship_binding
        .operations
        .contains_key("list_for_subject")
    {
        relationship_binding.operations.insert(
            "list_for_subject".into(),
            format_list_for_subject(adapter_type, &list_columns, &object_table, &subject_link_column),
        );
    }
}

// Build dialect-specific count query for a relationship.
fn format_count_for_subject(adapter_type: &str, object_table: &str, subject_link_column: &str) -> String {
    match adapter_type {
        "mysql" => format!(
            "SELECT COUNT(*) AS count FROM {object_table} WHERE {subject_link_column} = :subject_id"
        ),
        "mssql" => format!(
            "SELECT COUNT(*) AS count FROM {object_table} WHERE {subject_link_column} = :subject_id"
        ),
        "soql" => format!(
            "SELECT COUNT() FROM {object_table} WHERE {subject_link_column} = :subject_id"
        ),
        _ => format!(
            "SELECT COUNT(*)::bigint AS count FROM {object_table} WHERE {subject_link_column} = :subject_id"
        ),
    }
}

// Build dialect-specific list query for a relationship.
fn format_list_for_subject(
    adapter_type: &str,
    list_columns: &str,
    object_table: &str,
    subject_link_column: &str,
) -> String {
    match adapter_type {
        "mssql" => format!(
            "SELECT TOP (:limit) {list_columns} FROM {object_table} WHERE {subject_link_column} = :subject_id"
        ),
        "soql" => format!(
            "SELECT {list_columns} FROM {object_table} WHERE {subject_link_column} = :subject_id LIMIT :limit"
        ),
        _ => format!(
            "SELECT {list_columns} FROM {object_table} WHERE {subject_link_column} = :subject_id LIMIT :limit"
        ),
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

// Compile MongoDB find/count operation templates from declarative entity metadata.
fn compile_mongodb_binding_queries(binding: &mut PlaybookBinding) {
    let object_link_columns = collect_object_link_columns(binding);

    for (entity_name, entity_binding) in binding.entities.iter_mut() {
        let collection_name = match entity_binding.from.as_ref() {
            Some(value) if !value.trim().is_empty() => value.trim().to_string(),
            _ => continue,
        };

        if !entity_binding.operations.contains_key("list_all") {
            entity_binding.operations.insert(
                "list_all".into(),
                format!("find:{collection_name}:{{}}"),
            );
        }

        if !entity_binding.operations.contains_key("list_entity") {
            entity_binding.operations.insert(
                "list_entity".into(),
                format!("find:{collection_name}:{{}}:limit=:limit"),
            );
        }

        let id_field = resolve_physical_field(entity_binding, &entity_binding.id_field);
        if !entity_binding.lookup.contains_key("by_identifier") {
            entity_binding.lookup.insert(
                "by_identifier".into(),
                format!("find:{collection_name}:{{\"{id_field}\":\":identifier\"}}"),
            );
        }

        if !entity_binding.lookup.contains_key("by_name") {
            if let Some(name_field) = resolve_name_column(entity_binding) {
                entity_binding.lookup.insert(
                    "by_name".into(),
                    format!(
                        "find:{collection_name}:{{\"{name_field}\":{{\"$regex\":\".*:name.*\",\"$options\":\"i\"}}}}"
                    ),
                );
            }
        }

        if let Some(extra_columns) = object_link_columns.get(entity_name) {
            for extra_column in extra_columns {
                let _ = extra_column;
            }
        }
    }

    for relationship_binding in binding.relationships.values_mut() {
        let subject_link_column = match relationship_binding.subject_link_column.as_ref() {
            Some(value) if !value.trim().is_empty() => value.trim().to_string(),
            _ => continue,
        };
        let object_entity_name = match relationship_binding.join.as_ref() {
            Some(join) => join.to_entity.clone(),
            None => continue,
        };
        let object_binding = match binding.entities.get(&object_entity_name) {
            Some(value) => value,
            None => continue,
        };
        let collection_name = match object_binding.from.as_ref() {
            Some(value) if !value.trim().is_empty() => value.trim().to_string(),
            _ => continue,
        };

        if !relationship_binding
            .operations
            .contains_key("count_for_subject")
        {
            relationship_binding.operations.insert(
                "count_for_subject".into(),
                format!("count:{collection_name}:{{\"{subject_link_column}\":\":subject_id\"}}"),
            );
        }

        if !relationship_binding
            .operations
            .contains_key("list_for_subject")
        {
            relationship_binding.operations.insert(
                "list_for_subject".into(),
                format!(
                    "find:{collection_name}:{{\"{subject_link_column}\":\":subject_id\"}}:limit=:limit"
                ),
            );
        }
    }
}

// Compile REST HTTP operation templates from declarative entity metadata.
fn compile_rest_binding_queries(binding: &mut PlaybookBinding) {
    for entity_binding in binding.entities.values_mut() {
        let resource_path = match entity_binding.from.as_ref() {
            Some(value) if !value.trim().is_empty() => normalize_rest_path(value.trim()),
            _ => continue,
        };

        if !entity_binding.operations.contains_key("list_all") {
            entity_binding.operations.insert(
                "list_all".into(),
                format!("GET {resource_path}"),
            );
        }

        if !entity_binding.operations.contains_key("list_entity") {
            entity_binding.operations.insert(
                "list_entity".into(),
                format!("GET {resource_path}?limit=:limit"),
            );
        }

        let id_field = resolve_physical_field(entity_binding, &entity_binding.id_field);
        if !entity_binding.lookup.contains_key("by_identifier") {
            entity_binding.lookup.insert(
                "by_identifier".into(),
                format!("GET {resource_path}/:{id_field}"),
            );
        }

        if !entity_binding.lookup.contains_key("by_name") {
            if let Some(name_field) = resolve_name_column(entity_binding) {
                entity_binding.lookup.insert(
                    "by_name".into(),
                    format!("GET {resource_path}?{name_field}=:name"),
                );
            }
        }
    }

    for relationship_binding in binding.relationships.values_mut() {
        let subject_link_column = match relationship_binding.subject_link_column.as_ref() {
            Some(value) if !value.trim().is_empty() => value.trim().to_string(),
            _ => continue,
        };
        let object_entity_name = match relationship_binding.join.as_ref() {
            Some(join) => join.to_entity.clone(),
            None => continue,
        };
        let object_binding = match binding.entities.get(&object_entity_name) {
            Some(value) => value,
            None => continue,
        };
        let resource_path = match object_binding.from.as_ref() {
            Some(value) if !value.trim().is_empty() => normalize_rest_path(value.trim()),
            _ => continue,
        };

        if !relationship_binding
            .operations
            .contains_key("count_for_subject")
        {
            relationship_binding.operations.insert(
                "count_for_subject".into(),
                format!("GET {resource_path}?{subject_link_column}=:subject_id"),
            );
        }

        if !relationship_binding
            .operations
            .contains_key("list_for_subject")
        {
            relationship_binding.operations.insert(
                "list_for_subject".into(),
                format!("GET {resource_path}?{subject_link_column}=:subject_id&limit=:limit"),
            );
        }
    }
}

// Resolve playbook field to physical column/JSON field name.
fn resolve_physical_field(entity_binding: &EntityBinding, playbook_field: &str) -> String {
    entity_binding
        .fields
        .get(playbook_field)
        .cloned()
        .unwrap_or_else(|| playbook_field.to_string())
}

// Ensure REST resource paths start with a slash.
fn normalize_rest_path(raw_path: &str) -> String {
    if raw_path.starts_with('/') {
        raw_path.to_string()
    } else {
        format!("/{raw_path}")
    }
}

// Validate binding entity/relationship names against a playbook definition.
pub fn validate_binding_for_playbook(
    playbook: &PlaybookDefinition,
    binding: &PlaybookBinding,
) -> BindingValidationReport {
    let mut compiled_binding = binding.clone();
    crate::normalize_relationship_bindings(&mut compiled_binding);
    crate::merge_entity_fields_from_playbook(&mut compiled_binding, playbook);
    crate::merge_relationships_from_playbook(&mut compiled_binding, playbook);
    compile_binding_queries(&mut compiled_binding);

    let binding = &compiled_binding;
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
        if !binding.entities.contains_key(&relationship.object_entity_name) {
            continue;
        }

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
        warnings.push("binding adapter is not set; infer from profile or file stem before execute".into());
    }

    for read_only_error in crate::read_only::validate_read_only_binding_queries(binding) {
        errors.push(read_only_error);
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
