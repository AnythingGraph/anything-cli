use serde_json::Value;
use thiserror::Error;

use playbook_spec::PlaybookDefinition;

use crate::types::RelationshipAccessRules;

#[derive(Debug, Error)]
pub enum RebacLoadError {
    #[error("json parse error: {0}")]
    ParseJson(#[from] serde_json::Error),
    #[error("playbook has no relationship_access_rules block")]
    MissingRules,
}

/// Parse relationship access rules from a playbook definition.
pub fn rules_from_playbook(playbook: &PlaybookDefinition) -> Result<Option<RelationshipAccessRules>, RebacLoadError> {
    let Some(raw_rules) = playbook.relationship_access_rules.as_ref() else {
        return Ok(None);
    };
    let rules: RelationshipAccessRules = serde_json::from_value(raw_rules.clone())?;
    Ok(Some(rules))
}

/// Parse relationship access rules from raw JSON value.
pub fn parse_relationship_access_rules(
    raw_rules: &Value,
) -> Result<RelationshipAccessRules, serde_json::Error> {
    serde_json::from_value(raw_rules.clone())
}

/// Return enforced rules only when `active` is true (or legacy implementation_status).
pub fn enforced_rules(rules: &RelationshipAccessRules) -> Option<RelationshipAccessRules> {
    if rules.is_enforced() {
        Some(rules.clone())
    } else {
        None
    }
}
