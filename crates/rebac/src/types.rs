use serde::{Deserialize, Serialize};

/// Read or write action referenced by playbook relationship access rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RebacAction {
    Read,
    Write,
}

/// Allow or deny effect on a matching resource row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RebacEffect {
    Allow,
    Deny,
}

/// Whether rules are enforced at runtime or catalog-only documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RebacImplementationStatus {
    CatalogOnly,
    Enforced,
}

/// One hop in a relationship-based access path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationshipAccessPathStep {
    pub relationship_name: String,
    pub direction: PathDirection,
    pub from_entity_name: String,
    pub to_entity_name: String,
}

/// Traverse a schema relationship forward (subject → object) or reverse (object → subject).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PathDirection {
    Forward,
    Reverse,
}

/// Condition evaluated against the access subject row before a rule applies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubjectCondition {
    pub field: String,
    pub operator: SubjectConditionOperator,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubjectConditionOperator {
    Equals,
    #[serde(rename = "in")]
    In,
}

/// Direct field comparison between a resource row and the access subject row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldMatchRule {
    #[serde(rename = "type")]
    pub match_type: FieldMatchType,
    pub resource_field: String,
    pub subject_field: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldMatchType {
    FieldEqualsSubject,
}

/// One relationship-based access rule from playbook JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationshipAccessRule {
    pub id: String,
    pub name: String,
    pub effect: RebacEffect,
    pub action: RebacAction,
    pub resource_entity_name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_condition: Option<SubjectCondition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<Vec<RelationshipAccessPathStep>>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "match")]
    pub match_rule: Option<FieldMatchRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_prior_access_to: Option<String>,
}

/// Playbook `relationship_access_rules` block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationshipAccessRules {
    pub summary: String,
    pub subject_entity_name: String,
    pub subject_identifier_field: String,
    pub rules: Vec<RelationshipAccessRule>,
    #[serde(default)]
    pub deny_by_default: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implementation_status: Option<RebacImplementationStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implementation_note: Option<String>,
}

impl RelationshipAccessRules {
    /// True when playbook rules should be applied at runtime.
    pub fn is_enforced(&self) -> bool {
        matches!(
            self.implementation_status,
            Some(RebacImplementationStatus::Enforced)
        )
    }
}

/// Resolved access subject used during evaluation (business identifier, not numeric row id).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubjectContext {
    pub entity_name: String,
    pub identifier_value: String,
}

/// Stable row key for graph evaluation (entity + business identifier).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RowKey {
    pub entity_name: String,
    pub row_id: String,
}
