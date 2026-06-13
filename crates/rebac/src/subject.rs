use thiserror::Error;

use crate::graph::{field_value_as_string, RebacGraphView};
use crate::types::{RelationshipAccessRules, SubjectContext};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SubjectResolveError {
    #[error("subject identifier must not be empty")]
    EmptySubjectIdentifier,
    #[error("no row found for subject entity '{entity_name}' where '{identifier_field}' = '{identifier_value}'")]
    SubjectNotFound {
        entity_name: String,
        identifier_field: String,
        identifier_value: String,
    },
}

/// Resolve the access subject from an external identifier value.
pub fn resolve_subject(
    rules: &RelationshipAccessRules,
    graph: &dyn RebacGraphView,
    subject_identifier_value: &str,
) -> Result<SubjectContext, SubjectResolveError> {
    let trimmed_identifier = subject_identifier_value.trim();
    if trimmed_identifier.is_empty() {
        return Err(SubjectResolveError::EmptySubjectIdentifier);
    }

    let subject_entity_name = rules.subject_entity_name.as_str();
    let identifier_field = rules.subject_identifier_field.as_str();

    for row_key in graph.list_row_keys(subject_entity_name) {
        let Some(row_snapshot) = graph.row_snapshot(&row_key) else {
            continue;
        };
        let Some(row_identifier) = field_value_as_string(&row_snapshot.values, identifier_field)
        else {
            continue;
        };
        if row_identifier.trim() == trimmed_identifier {
            return Ok(SubjectContext {
                entity_name: subject_entity_name.to_string(),
                identifier_value: trimmed_identifier.to_string(),
            });
        }
    }

    Err(SubjectResolveError::SubjectNotFound {
        entity_name: subject_entity_name.to_string(),
        identifier_field: identifier_field.to_string(),
        identifier_value: trimmed_identifier.to_string(),
    })
}
