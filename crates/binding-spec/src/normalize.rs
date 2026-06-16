use playbook_spec::PlaybookDefinition;

use crate::{PlaybookBinding, RelationshipJoin, SourceProfile};

// Infer adapter type from binding file stem suffix (e.g. crm-payroll-access.postgres → sql).
pub fn infer_adapter_from_binding_stem(binding_stem: &str) -> Option<String> {
    let suffix = binding_stem
        .rsplit('.')
        .next()
        .unwrap_or(binding_stem)
        .trim()
        .to_ascii_lowercase();

    match suffix.as_str() {
        "postgres" | "pg" => Some("sql".to_string()),
        "csv" => Some("csv".to_string()),
        "salesforce" | "soql" => Some("soql".to_string()),
        "mysql" => Some("mysql".to_string()),
        "mssql" => Some("mssql".to_string()),
        "mongodb" | "mongo" => Some("mongodb".to_string()),
        "rest" | "openapi" => Some("rest".to_string()),
        "sql" => Some("sql".to_string()),
        _ => None,
    }
}

// Infer playbook id from binding stem when using playbook-scoped filenames.
pub fn playbook_id_from_binding_stem(binding_stem: &str) -> Option<String> {
    let mut parts = binding_stem.split('.');
    let first = parts.next();
    let second = parts.next();
    if let (Some(playbook_id), Some(suffix)) = (first, second) {
        if infer_adapter_from_binding_stem(suffix).is_some() {
            return Some(playbook_id.to_string());
        }
    }
    None
}

// Resolve adapter on a binding using explicit field, profile source, or file stem.
pub fn infer_binding_adapter(
    binding: &PlaybookBinding,
    profile: Option<&SourceProfile>,
    binding_stem: &str,
) -> Option<String> {
    if !binding.adapter.trim().is_empty() {
        return Some(binding.adapter.trim().to_string());
    }

    if let Some(source_id) = binding.source_id.as_ref() {
        if let Some(profile) = profile {
            if let Some(source) = profile.sources.get(source_id) {
                if !source.adapter.trim().is_empty() {
                    return Some(source.adapter.clone());
                }
            }
        }
    }

    infer_adapter_from_binding_stem(binding_stem)
}

// Normalize compact relationship fields (object, link_column) into join metadata.
pub fn normalize_relationship_bindings(binding: &mut PlaybookBinding) {
    for relationship_binding in binding.relationships.values_mut() {
        if relationship_binding.join.is_none() {
            if let Some(object_entity) = relationship_binding.object.clone() {
                relationship_binding.join = Some(RelationshipJoin {
                    from_entity: String::new(),
                    to_entity: object_entity,
                    on: String::new(),
                });
            }
        }

        if relationship_binding.subject_link_column.is_none() {
            relationship_binding.subject_link_column = relationship_binding.link_column.clone();
        }
    }
}

// Fill same-name playbook fields when binding fields map is sparse.
pub fn merge_entity_fields_from_playbook(
    binding: &mut PlaybookBinding,
    playbook: &PlaybookDefinition,
) {
    for (entity_name, entity_binding) in binding.entities.iter_mut() {
        let playbook_entity = playbook
            .entities
            .iter()
            .find(|entity| entity.name == *entity_name);

        if let Some(playbook_entity) = playbook_entity {
            let identifier_field = playbook_entity
                .fields
                .iter()
                .find(|field| field.is_identifier)
                .map(|field| field.field_name.clone());

            if let Some(identifier_field) = identifier_field {
                if entity_binding.id_field.trim().is_empty() {
                    entity_binding.id_field = identifier_field.clone();
                }
            }

            for field in &playbook_entity.fields {
                if field.is_identifier {
                    continue;
                }
                if !entity_binding.fields.contains_key(&field.field_name) {
                    entity_binding.fields.insert(
                        field.field_name.clone(),
                        field.field_name.clone(),
                    );
                }
            }
        }
    }
}

// Inject playbook relationships into bindings that own the object entity (cross-source federation).
pub fn merge_relationships_from_playbook(
    binding: &mut PlaybookBinding,
    playbook: &PlaybookDefinition,
) {
    for relationship in &playbook.entity_relationships {
        let object_entity_name = &relationship.object_entity_name;
        let object_entity_binding = match binding.entities.get(object_entity_name) {
            Some(value) => value,
            None => continue,
        };

        if binding
            .relationships
            .contains_key(&relationship.relationship_name)
        {
            continue;
        }

        let join_playbook_field = resolve_relationship_join_field(playbook, relationship);
        let physical_link_column =
            physical_field_for_playbook_field(object_entity_binding, &join_playbook_field);

        binding.relationships.insert(
            relationship.relationship_name.clone(),
            crate::RelationshipBinding {
                join: Some(crate::RelationshipJoin {
                    from_entity: relationship.subject_entity_name.clone(),
                    to_entity: object_entity_name.clone(),
                    on: join_playbook_field,
                }),
                object: Some(object_entity_name.clone()),
                link_column: Some(physical_link_column.clone()),
                subject_link_column: Some(physical_link_column),
                operations: std::collections::HashMap::new(),
            },
        );
    }
}

// Pick the playbook field used to join a resolved subject to object rows.
fn resolve_relationship_join_field(
    playbook: &PlaybookDefinition,
    relationship: &playbook_spec::PlaybookEntityRelationship,
) -> String {
    if let Some(join_on) = relationship.join_on.as_ref() {
        let trimmed = join_on.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    identifier_field_for_entity(playbook, &relationship.subject_entity_name)
        .or_else(|| identifier_field_for_entity(playbook, &relationship.object_entity_name))
        .unwrap_or_else(|| "id".to_string())
}

// Return the playbook identifier field name for one entity.
fn identifier_field_for_entity(playbook: &PlaybookDefinition, entity_name: &str) -> Option<String> {
    playbook
        .entities
        .iter()
        .find(|entity| entity.name == entity_name)
        .and_then(|entity| {
            entity
                .fields
                .iter()
                .find(|field| field.is_identifier)
                .map(|field| field.field_name.clone())
        })
}

// Map a playbook field to the physical column/property on an entity binding.
fn physical_field_for_playbook_field(
    entity_binding: &crate::EntityBinding,
    playbook_field: &str,
) -> String {
    entity_binding
        .fields
        .get(playbook_field)
        .cloned()
        .unwrap_or_else(|| {
            if entity_binding.id_field == playbook_field {
                playbook_field.to_string()
            } else {
                entity_binding
                    .fields
                    .get(&entity_binding.id_field)
                    .cloned()
                    .unwrap_or_else(|| entity_binding.id_field.clone())
            }
        })
}

// Finalize binding: infer adapter, normalize shape, merge playbook fields, compile queries.
pub fn finalize_binding(
    binding: &mut PlaybookBinding,
    profile: Option<&SourceProfile>,
    binding_stem: &str,
    playbook: Option<&PlaybookDefinition>,
) {
    if let Some(adapter) = infer_binding_adapter(binding, profile, binding_stem) {
        binding.adapter = adapter;
    }

    normalize_relationship_bindings(binding);

    if let Some(playbook) = playbook {
        merge_entity_fields_from_playbook(binding, playbook);
        merge_relationships_from_playbook(binding, playbook);
    }

    crate::compile::compile_binding_queries(binding);
}
