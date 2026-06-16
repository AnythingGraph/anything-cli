mod compile;
mod normalize;
mod read_only;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use compile::{
    compile_binding_queries, playbook_binding_stem, suggest_entity_table_mappings,
    validate_binding_for_playbook, BindingValidationReport,
};
pub use read_only::{read_only_query_violation, validate_read_only_binding_queries};
pub use normalize::{
    finalize_binding, infer_adapter_from_binding_stem, infer_binding_adapter,
    merge_entity_fields_from_playbook, merge_relationships_from_playbook,
    normalize_relationship_bindings, playbook_id_from_binding_stem,
};

#[derive(Debug, Error)]
pub enum BindingError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("binding error: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceProfile {
    pub sources: HashMap<String, SourceConnection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConnection {
    pub adapter: String,
    #[serde(default)]
    pub dsn: Option<String>,
    #[serde(default)]
    pub instance_url: Option<String>,
    #[serde(default)]
    pub auth: Option<String>,
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookBinding {
    #[serde(default)]
    pub adapter: String,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub playbook_id: Option<String>,
    #[serde(default)]
    pub source_id: Option<String>,
    pub entities: HashMap<String, EntityBinding>,
    #[serde(default)]
    pub relationships: HashMap<String, RelationshipBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityBinding {
    #[serde(default)]
    pub from: Option<String>,
    #[serde(alias = "id")]
    pub id_field: String,
    #[serde(default, deserialize_with = "deserialize_entity_fields")]
    pub fields: HashMap<String, String>,
    #[serde(default)]
    pub lookup: HashMap<String, String>,
    #[serde(default)]
    pub operations: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipBinding {
    #[serde(default)]
    pub join: Option<RelationshipJoin>,
    #[serde(default)]
    pub object: Option<String>,
    #[serde(default)]
    pub link_column: Option<String>,
    #[serde(default)]
    pub subject_link_column: Option<String>,
    #[serde(default)]
    pub operations: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipJoin {
    #[serde(default)]
    pub from_entity: String,
    pub to_entity: String,
    #[serde(default)]
    pub on: String,
}

// Accept entity fields as a list of same-name columns or a playbook→physical map.
fn deserialize_entity_fields<'de, D>(
    deserializer: D,
) -> Result<HashMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw_value = serde_yaml::Value::deserialize(deserializer)?;
    match raw_value {
        serde_yaml::Value::Sequence(items) => {
            let mut fields = HashMap::new();
            for item in items {
                if let serde_yaml::Value::String(field_name) = item {
                    fields.insert(field_name.clone(), field_name);
                }
            }
            Ok(fields)
        }
        serde_yaml::Value::Mapping(map) => {
            let mut fields = HashMap::new();
            for (key, value) in map {
                let field_name = key.as_str().unwrap_or_default().to_string();
                if field_name.is_empty() {
                    continue;
                }
                let physical_name = match value {
                    serde_yaml::Value::String(physical) => physical,
                    _ => field_name.clone(),
                };
                fields.insert(field_name, physical_name);
            }
            Ok(fields)
        }
        serde_yaml::Value::Null => Ok(HashMap::new()),
        _ => Err(serde::de::Error::custom(
            "entity fields must be a list or map",
        )),
    }
}

// Load a binding YAML file from disk.
pub fn load_binding_from_path(binding_path: &Path) -> Result<PlaybookBinding, BindingError> {
    let raw_text = fs::read_to_string(binding_path)?;
    load_binding_from_yaml(&raw_text)
}

// Parse binding YAML without inferring adapter or compiling queries.
pub fn load_binding_from_yaml(raw_yaml: &str) -> Result<PlaybookBinding, BindingError> {
    let mut binding: PlaybookBinding = serde_yaml::from_str(raw_yaml)?;
    normalize_relationship_bindings(&mut binding);
    Ok(binding)
}

// Serialize binding to YAML for agents to review or save.
pub fn binding_to_yaml(binding: &PlaybookBinding) -> Result<String, BindingError> {
    serde_yaml::to_string(binding).map_err(BindingError::Yaml)
}

// Write binding YAML to the bindings directory (serializes full in-memory binding — compiled form).
pub fn save_binding_to_path(binding_path: &Path, binding: &PlaybookBinding) -> Result<(), BindingError> {
    if let Some(parent) = binding_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let yaml_text = binding_to_yaml(binding)?;
    fs::write(binding_path, yaml_text)?;
    Ok(())
}

// Persist agent-authored binding YAML exactly as submitted (after validation), not compiled output.
pub fn save_binding_authoring_yaml_to_path(
    binding_path: &Path,
    binding_yaml: &str,
) -> Result<(), BindingError> {
    if let Some(parent) = binding_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut yaml_text = binding_yaml.trim().to_string();
    if !yaml_text.is_empty() {
        yaml_text.push('\n');
    }
    fs::write(binding_path, yaml_text)?;
    Ok(())
}

// Resolve on-disk path for a playbook-scoped binding file.
pub fn playbook_binding_path(bindings_dir: &Path, playbook_id: &str, adapter_suffix: &str) -> PathBuf {
    bindings_dir.join(format!("{}.yaml", playbook_binding_stem(playbook_id, adapter_suffix)))
}

// Load a profile YAML file (credentials and source registry).
pub fn load_profile_from_path(profile_path: &Path) -> Result<SourceProfile, BindingError> {
    let raw_text = fs::read_to_string(profile_path)?;
    let profile: SourceProfile = serde_yaml::from_str(&raw_text)?;
    Ok(profile)
}

// Resolve which adapter type to use for a binding + profile.
pub fn resolve_adapter_type(
    binding: &PlaybookBinding,
    profile: &SourceProfile,
) -> Result<String, BindingError> {
    if let Some(adapter) = infer_binding_adapter(binding, Some(profile), "") {
        return Ok(adapter);
    }
    if !binding.adapter.trim().is_empty() {
        return Ok(binding.adapter.clone());
    }
    Err(BindingError::Invalid(
        "could not resolve adapter type for binding".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;

    #[test]
    fn save_binding_authoring_yaml_preserves_compact_input() {
        let temp_dir = std::env::temp_dir().join(format!(
            "ag-binding-authoring-{}",
            std::process::id()
        ));
        let binding_path = temp_dir.join("demo.csv.yaml");
        let _ = fs::remove_dir_all(&temp_dir);

        let compact_yaml = r#"source_id: payroll_csv

entities:
  employee:
    from: payroll.csv
    id: user_id
    fields:
      user_id: user

relationships:
  employee_has_payroll:
    object: payroll_record
    link_column: user
"#;

        save_binding_authoring_yaml_to_path(&binding_path, compact_yaml).expect("save authoring yaml");
        let written = fs::read_to_string(&binding_path).expect("read saved binding");
        assert!(written.contains("source_id: payroll_csv"));
        assert!(written.contains("link_column: user"));
        assert!(!written.contains("lookup:"));
        assert!(!written.contains("operations:"));
        assert!(!written.contains("SELECT"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn merge_relationships_injects_object_entity_join_on_identifier() {
        let playbook = playbook_spec::PlaybookDefinition {
            id: "demo".into(),
            name: "Demo".into(),
            description: String::new(),
            category: String::new(),
            instructions: None,
            entities: vec![
                playbook_spec::PlaybookEntity {
                    name: "customer".into(),
                    display_name: "Customer".into(),
                    fields: vec![playbook_spec::PlaybookField {
                        field_name: "user_id".into(),
                        field_type: String::new(),
                        is_identifier: true,
                    }],
                },
                playbook_spec::PlaybookEntity {
                    name: "crm_user".into(),
                    display_name: "CRM user".into(),
                    fields: vec![playbook_spec::PlaybookField {
                        field_name: "user_id".into(),
                        field_type: String::new(),
                        is_identifier: true,
                    }],
                },
            ],
            entity_relationships: vec![playbook_spec::PlaybookEntityRelationship {
                relationship_name: "same_person".into(),
                subject_entity_name: "customer".into(),
                object_entity_name: "crm_user".into(),
                join_on: None,
            }],
            relationship_access_rules: None,
            entity_sources: None,
            bindings: None,
            default_binding: None,
            field_mappings: None,
        };

        let mut postgres_binding = PlaybookBinding {
            adapter: "sql".into(),
            version: 0,
            playbook_id: None,
            source_id: None,
            entities: HashMap::from([(
                "crm_user".into(),
                EntityBinding {
                    from: Some("users".into()),
                    id_field: "user_id".into(),
                    fields: HashMap::from([("user_id".into(), "user_id".into())]),
                    lookup: HashMap::new(),
                    operations: HashMap::new(),
                },
            )]),
            relationships: HashMap::new(),
        };

        merge_relationships_from_playbook(&mut postgres_binding, &playbook);
        compile_binding_queries(&mut postgres_binding);

        let relationship = postgres_binding
            .relationships
            .get("same_person")
            .expect("relationship should be injected");
        assert_eq!(
            relationship.subject_link_column.as_deref(),
            Some("user_id")
        );
        assert!(relationship
            .operations
            .contains_key("count_for_subject"));
    }

    #[test]
    fn compile_binding_adds_list_entity_operation() {
        let mut binding = PlaybookBinding {
            adapter: "sql".into(),
            version: 0,
            playbook_id: None,
            source_id: None,
            entities: HashMap::from([(
                "crm_user".into(),
                EntityBinding {
                    from: Some("users".into()),
                    id_field: "user_id".into(),
                    fields: HashMap::from([("user_id".into(), "user_id".into())]),
                    lookup: HashMap::new(),
                    operations: HashMap::new(),
                },
            )]),
            relationships: HashMap::new(),
        };

        compile_binding_queries(&mut binding);
        let list_entity = binding.entities["crm_user"]
            .operations
            .get("list_entity")
            .expect("list_entity operation");
        assert!(list_entity.contains("LIMIT :limit"));
    }
}
