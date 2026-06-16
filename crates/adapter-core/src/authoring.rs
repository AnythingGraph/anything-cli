use serde::{Deserialize, Serialize};

/// Agent-facing binding authoring guide for one adapter type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterAuthoringGuide {
    pub adapter: String,
    /// Playbook binding filename suffix, e.g. postgres, csv.
    pub binding_file_suffix: String,
    /// What `entities.*.from` maps to in the physical source.
    pub entity_from_meaning: String,
    /// When set, describes introspect_source `schema_name` (MCP API only — not binding YAML).
    pub introspect_schema_name: Option<String>,
    /// Top-level binding YAML keys that must not appear (silently ignored today).
    pub forbidden_binding_keys: Vec<String>,
    /// Allowed top-level binding YAML keys for compact authoring.
    pub allowed_top_level_keys: Vec<String>,
    /// Full markdown instructions from the adapter crate AGENTS.md.
    pub instructions_markdown: String,
    /// Minimal example binding YAML snippet.
    pub example_binding_yaml: Option<String>,
    /// Recommended MCP steps after list_sources for this source.
    pub workflow_steps: Vec<String>,
}
