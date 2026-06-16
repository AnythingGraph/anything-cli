use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    PlaybookDefinition, PlaybookEntity, PlaybookEntityRelationship, PlaybookError, PlaybookField,
};

#[derive(Debug, Deserialize)]
struct RawPlaybookDocument {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    instructions: Option<String>,
    #[serde(default)]
    entities: Value,
    #[serde(default)]
    entity_relationships: Vec<PlaybookEntityRelationship>,
    #[serde(default)]
    relationships: Value,
    #[serde(default)]
    sources: HashMap<String, String>,
    #[serde(default)]
    entity_sources: HashMap<String, String>,
    #[serde(default)]
    bindings: HashMap<String, String>,
    #[serde(default)]
    default_binding: Option<String>,
    #[serde(default)]
    access: Option<AccessBlock>,
    #[serde(default)]
    relationship_access_rules: Option<Value>,
    #[serde(default)]
    field_mappings: Option<
        HashMap<String, HashMap<String, HashMap<String, String>>>,
    >,
}

#[derive(Debug, Deserialize)]
struct AccessBlock {
    subject: String,
    subject_id: String,
    #[serde(default = "default_true")]
    deny_by_default: bool,
    #[serde(default = "default_true")]
    active: bool,
    allow: Vec<AccessAllowRule>,
    #[serde(default)]
    summary: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct AccessAllowRule {
    relationship: String,
    resource: String,
}

#[derive(Debug, Deserialize)]
struct CompactEntity {
    identifier: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    attributes: Value,
}

// Parse playbook JSON (compact or legacy) into canonical PlaybookDefinition.
pub fn normalize_playbook_document(
    raw_text: &str,
    fallback_id: &str,
) -> Result<PlaybookDefinition, PlaybookError> {
    let raw: RawPlaybookDocument = serde_json::from_str(raw_text)?;
    let playbook_id = raw.id.trim();
    let playbook_id = if playbook_id.is_empty() {
        fallback_id.to_string()
    } else {
        playbook_id.to_string()
    };

    let name = if raw.name.trim().is_empty() {
        humanize_identifier(&playbook_id)
    } else {
        raw.name
    };

    let entities = parse_entities(&raw.entities)?;
    let entity_relationships = parse_relationships(&raw.relationships, &raw.entity_relationships)?;

    let (entity_sources, bindings) = merge_source_routing(
        &playbook_id,
        &entities,
        raw.sources,
        raw.entity_sources,
        raw.bindings,
    );

    let relationship_access_rules = if let Some(access) = raw.access {
        Some(expand_access_block(&access, &entity_relationships)?)
    } else {
        raw.relationship_access_rules
    };

    Ok(PlaybookDefinition {
        id: playbook_id,
        name,
        description: raw.description,
        category: raw.category,
        instructions: raw.instructions,
        entities,
        entity_relationships,
        relationship_access_rules,
        entity_sources: Some(entity_sources),
        bindings: Some(bindings),
        default_binding: raw.default_binding,
        field_mappings: raw.field_mappings,
    })
}

fn parse_entities(raw_entities: &Value) -> Result<Vec<PlaybookEntity>, PlaybookError> {
    if raw_entities.is_null() {
        return Ok(Vec::new());
    }

    if let Some(array) = raw_entities.as_array() {
        let mut entities = Vec::new();
        for item in array {
            let entity: PlaybookEntity = serde_json::from_value(item.clone())
                .map_err(|error| PlaybookError::Validation(error.to_string()))?;
            entities.push(normalize_legacy_entity(entity));
        }
        return Ok(entities);
    }

    if let Some(map) = raw_entities.as_object() {
        let mut entities = Vec::new();
        for (entity_name, entity_value) in map {
            let compact: CompactEntity = serde_json::from_value(entity_value.clone())
                .map_err(|error| PlaybookError::Validation(error.to_string()))?;
            entities.push(compact_entity_to_playbook_entity(entity_name, compact)?);
        }
        entities.sort_by(|left, right| left.name.cmp(&right.name));
        return Ok(entities);
    }

    Err(PlaybookError::Validation(
        "entities must be an array or object map".into(),
    ))
}

fn normalize_legacy_entity(entity: PlaybookEntity) -> PlaybookEntity {
    let display_name = if entity.display_name.trim().is_empty() {
        humanize_identifier(&entity.name)
    } else {
        entity.display_name
    };
    PlaybookEntity {
        name: entity.name,
        display_name,
        fields: entity.fields,
    }
}

fn compact_entity_to_playbook_entity(
    entity_name: &str,
    compact: CompactEntity,
) -> Result<PlaybookEntity, PlaybookError> {
    let display_name = compact
        .display_name
        .unwrap_or_else(|| humanize_identifier(entity_name));

    let identifier_field = compact.identifier.trim();
    if identifier_field.is_empty() {
        return Err(PlaybookError::Validation(format!(
            "entity '{entity_name}' requires a non-empty identifier"
        )));
    }
    let identifier_field = identifier_field.to_string();

    let mut fields = Vec::new();
    fields.push(PlaybookField {
        field_name: identifier_field.clone(),
        field_type: String::new(),
        is_identifier: true,
    });

    match &compact.attributes {
        Value::Null => {}
        Value::Array(items) => {
            for item in items {
                if let Some(field_name) = item.as_str() {
                    if field_name != identifier_field {
                        fields.push(PlaybookField {
                            field_name: field_name.to_string(),
                            field_type: String::new(),
                            is_identifier: false,
                        });
                    }
                }
            }
        }
        Value::Object(map) => {
            for (field_name, field_value) in map {
                let is_identifier = field_name == &identifier_field;
                if is_identifier {
                    continue;
                }
                let description = field_value
                    .get("description")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string());
                let field_type = field_value
                    .get("field_type")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string();
                if description.is_some() || !field_type.is_empty() {
                    fields.push(PlaybookField {
                        field_name: field_name.clone(),
                        field_type,
                        is_identifier: false,
                    });
                } else if field_value.is_string() {
                    fields.push(PlaybookField {
                        field_name: field_name.clone(),
                        field_type: String::new(),
                        is_identifier: false,
                    });
                }
            }
        }
        _ => {
            return Err(PlaybookError::Validation(format!(
                "entity '{entity_name}' attributes must be an array or object"
            )));
        }
    }

    Ok(PlaybookEntity {
        name: entity_name.to_string(),
        display_name,
        fields,
    })
}

fn parse_relationships(
    compact_relationships: &Value,
    legacy_relationships: &[PlaybookEntityRelationship],
) -> Result<Vec<PlaybookEntityRelationship>, PlaybookError> {
    if !legacy_relationships.is_empty() {
        return Ok(legacy_relationships.to_vec());
    }

    if compact_relationships.is_null() {
        return Ok(Vec::new());
    }

    let map = compact_relationships.as_object().ok_or_else(|| {
        PlaybookError::Validation("relationships must be an object map".into())
    })?;

    let mut relationships = Vec::new();
    for (relationship_name, relationship_value) in map {
        let from = relationship_value
            .get("from")
            .or_else(|| relationship_value.get("subject_entity_name"))
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let to = relationship_value
            .get("to")
            .or_else(|| relationship_value.get("object_entity_name"))
            .and_then(|value| value.as_str())
            .unwrap_or_default();

        if from.is_empty() || to.is_empty() {
            return Err(PlaybookError::Validation(format!(
                "relationship '{relationship_name}' requires from and to"
            )));
        }

        relationships.push(PlaybookEntityRelationship {
            relationship_name: relationship_name.clone(),
            subject_entity_name: from.to_string(),
            object_entity_name: to.to_string(),
        });
    }

    relationships.sort_by(|left, right| left.relationship_name.cmp(&right.relationship_name));
    Ok(relationships)
}

fn merge_source_routing(
    playbook_id: &str,
    entities: &[PlaybookEntity],
    compact_sources: HashMap<String, String>,
    legacy_entity_sources: HashMap<String, String>,
    legacy_bindings: HashMap<String, String>,
) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut entity_sources = legacy_entity_sources;
    if !compact_sources.is_empty() {
        entity_sources = compact_sources;
    }

    let mut bindings = legacy_bindings;
    if bindings.is_empty() && !entity_sources.is_empty() {
        let mut source_keys: Vec<String> = entity_sources
            .values()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        source_keys.sort();
        for source_key in source_keys {
            bindings.insert(
                source_key.clone(),
                format!("{playbook_id}.{source_key}"),
            );
        }
    }

    for entity in entities {
        if !entity_sources.contains_key(&entity.name) {
            if let Some(default_source) = bindings.keys().next().cloned() {
                entity_sources.insert(entity.name.clone(), default_source);
            }
        }
    }

    (entity_sources, bindings)
}

fn expand_access_block(
    access: &AccessBlock,
    relationships: &[PlaybookEntityRelationship],
) -> Result<Value, PlaybookError> {
    let mut rules = Vec::new();

    for allow_rule in &access.allow {
        let relationship = relationships
            .iter()
            .find(|item| item.relationship_name == allow_rule.relationship)
            .ok_or_else(|| {
                PlaybookError::Validation(format!(
                    "access rule references unknown relationship '{}'",
                    allow_rule.relationship
                ))
            })?;

        if relationship.object_entity_name != allow_rule.resource {
            return Err(PlaybookError::Validation(format!(
                "access rule resource '{}' does not match relationship '{}' object '{}'",
                allow_rule.resource,
                allow_rule.relationship,
                relationship.object_entity_name
            )));
        }

        rules.push(json!({
            "id": allow_rule.relationship.clone(),
            "name": humanize_identifier(&allow_rule.relationship),
            "effect": "allow",
            "action": "read",
            "resource_entity_name": allow_rule.resource,
            "description": format!("Allowed via {}", allow_rule.relationship),
            "path": [{
                "relationship_name": relationship.relationship_name,
                "direction": "forward",
                "from_entity_name": relationship.subject_entity_name,
                "to_entity_name": relationship.object_entity_name,
            }],
        }));
    }

    let summary = access
        .summary
        .clone()
        .unwrap_or_else(|| format!("Access for {}", access.subject));

    Ok(json!({
        "summary": summary,
        "subject_entity_name": access.subject,
        "subject_identifier_field": access.subject_id,
        "deny_by_default": access.deny_by_default,
        "active": access.active,
        "rules": rules,
    }))
}

fn humanize_identifier(raw_value: &str) -> String {
    raw_value
        .replace('_', " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
