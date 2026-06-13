use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlaybookError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("validation error: {0}")]
    Validation(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub instructions: Option<String>,
    pub entities: Vec<PlaybookEntity>,
    #[serde(default)]
    pub entity_relationships: Vec<PlaybookEntityRelationship>,
    #[serde(default)]
    pub relationship_access_rules: Option<serde_json::Value>,
    #[serde(default)]
    pub entity_sources: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    pub bindings: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    pub default_binding: Option<String>,
    #[serde(default)]
    pub field_mappings: Option<std::collections::HashMap<String, std::collections::HashMap<String, std::collections::HashMap<String, String>>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookEntity {
    pub name: String,
    pub display_name: String,
    #[serde(default)]
    pub fields: Vec<PlaybookField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookField {
    pub field_name: String,
    #[serde(default)]
    pub field_type: String,
    #[serde(default)]
    pub is_identifier: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookEntityRelationship {
    pub relationship_name: String,
    pub subject_entity_name: String,
    pub object_entity_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub entity_count: usize,
    pub relationship_count: usize,
}

// Load one playbook JSON file from disk.
pub fn load_playbook_from_path(playbook_path: &Path) -> Result<PlaybookDefinition, PlaybookError> {
    let raw_text = fs::read_to_string(playbook_path)?;
    let mut playbook: PlaybookDefinition = serde_json::from_str(&raw_text)?;
    validate_playbook(&playbook)?;
    if playbook.id.trim().is_empty() {
        let fallback_id = playbook_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("playbook")
            .to_string();
        playbook.id = fallback_id;
    }
    Ok(playbook)
}

// Validate required playbook structure.
pub fn validate_playbook(playbook: &PlaybookDefinition) -> Result<(), PlaybookError> {
    if playbook.name.trim().is_empty() {
        return Err(PlaybookError::Validation("playbook name is required".into()));
    }
    if playbook.entities.is_empty() {
        return Err(PlaybookError::Validation(
            "playbook must include at least one entity".into(),
        ));
    }
    Ok(())
}

// Discover playbook JSON files in a directory (non-recursive).
pub fn discover_playbooks_in_directory(playbooks_dir: &Path) -> Result<Vec<PathBuf>, PlaybookError> {
    let mut paths = Vec::new();
    if !playbooks_dir.exists() {
        return Ok(paths);
    }
    for entry in fs::read_dir(playbooks_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

// Resolve binding file stem from entity_sources + bindings map.
pub fn resolve_binding_name_for_entity(
    playbook: &PlaybookDefinition,
    entity_name: &str,
) -> Option<String> {
    let bindings = playbook.bindings.as_ref()?;
    let source_key = playbook.entity_sources.as_ref()?.get(entity_name)?;
    bindings.get(source_key).cloned()
}

// Build a playbook context summary for agents and HTTP APIs.
pub fn playbook_context_summary(playbook: &PlaybookDefinition) -> PlaybookContextSummary {
    PlaybookContextSummary {
        id: playbook.id.clone(),
        name: playbook.name.clone(),
        description: playbook.description.clone(),
        entities: playbook
            .entities
            .iter()
            .map(|entity| PlaybookEntitySummary {
                name: entity.name.clone(),
                display_name: entity.display_name.clone(),
                identifier_field: entity
                    .fields
                    .iter()
                    .find(|field| field.is_identifier)
                    .map(|field| field.field_name.clone()),
            })
            .collect(),
        relationships: playbook
            .entity_relationships
            .iter()
            .map(|relationship| PlaybookRelationshipSummary {
                name: relationship.relationship_name.clone(),
                subject_entity_name: relationship.subject_entity_name.clone(),
                object_entity_name: relationship.object_entity_name.clone(),
            })
            .collect(),
        field_mappings: playbook.field_mappings.clone(),
        entity_sources: playbook.entity_sources.clone(),
        bindings: playbook.bindings.clone(),
        default_binding: playbook.default_binding.clone(),
        rebac_enforced: is_rebac_enforced(playbook),
        rebac_subject_entity: rebac_subject_entity_name(playbook),
    }
}

// True when playbook relationship_access_rules are enforced at runtime.
fn is_rebac_enforced(playbook: &PlaybookDefinition) -> bool {
    let Some(rules) = playbook.relationship_access_rules.as_ref() else {
        return false;
    };
    if rules
        .get("active")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return true;
    }
    rules
        .get("implementation_status")
        .and_then(|status| status.as_str())
        == Some("enforced")
}

// Subject entity name from relationship_access_rules when present.
fn rebac_subject_entity_name(playbook: &PlaybookDefinition) -> Option<String> {
    playbook
        .relationship_access_rules
        .as_ref()
        .and_then(|value| value.get("subject_entity_name"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookContextSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub entities: Vec<PlaybookEntitySummary>,
    pub relationships: Vec<PlaybookRelationshipSummary>,
    #[serde(default)]
    pub field_mappings: Option<std::collections::HashMap<String, std::collections::HashMap<String, std::collections::HashMap<String, String>>>>,
    #[serde(default)]
    pub entity_sources: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    pub bindings: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    pub default_binding: Option<String>,
    #[serde(default)]
    pub rebac_enforced: bool,
    #[serde(default)]
    pub rebac_subject_entity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookEntitySummary {
    pub name: String,
    pub display_name: String,
    pub identifier_field: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookRelationshipSummary {
    pub name: String,
    pub subject_entity_name: String,
    pub object_entity_name: String,
}
