mod compile;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use compile::{
    compile_binding_queries, playbook_binding_stem, suggest_entity_table_mappings,
    validate_binding_for_playbook, BindingValidationReport,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookBinding {
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
    pub id_field: String,
    #[serde(default)]
    pub fields: HashMap<String, String>,
    #[serde(default)]
    pub lookup: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipBinding {
    #[serde(default)]
    pub join: Option<RelationshipJoin>,
    #[serde(default)]
    pub subject_link_column: Option<String>,
    #[serde(default)]
    pub operations: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipJoin {
    pub from_entity: String,
    pub to_entity: String,
    pub on: String,
}

// Load a binding YAML file from disk.
pub fn load_binding_from_path(binding_path: &Path) -> Result<PlaybookBinding, BindingError> {
    let raw_text = fs::read_to_string(binding_path)?;
    load_binding_from_yaml(&raw_text)
}

// Parse binding YAML text and compile any declarative metadata into SQL.
pub fn load_binding_from_yaml(raw_yaml: &str) -> Result<PlaybookBinding, BindingError> {
    let mut binding: PlaybookBinding = serde_yaml::from_str(raw_yaml)?;
    if binding.adapter.trim().is_empty() {
        return Err(BindingError::Invalid("binding adapter is required".into()));
    }
    compile_binding_queries(&mut binding);
    Ok(binding)
}

// Serialize binding to YAML for agents to review or save.
pub fn binding_to_yaml(binding: &PlaybookBinding) -> Result<String, BindingError> {
    serde_yaml::to_string(binding).map_err(BindingError::Yaml)
}

// Write binding YAML to the bindings directory.
pub fn save_binding_to_path(binding_path: &Path, binding: &PlaybookBinding) -> Result<(), BindingError> {
    if let Some(parent) = binding_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let yaml_text = binding_to_yaml(binding)?;
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
    if let Some(source_id) = binding.source_id.as_ref() {
        let source = profile.sources.get(source_id).ok_or_else(|| {
            BindingError::Invalid(format!("profile missing source id: {source_id}"))
        })?;
        return Ok(source.adapter.clone());
    }
    Ok(binding.adapter.clone())
}
