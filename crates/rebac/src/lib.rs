//! Relationship-based access control for ag-cli playbooks.
//!
//! Self-contained ReBAC: load rules from playbook JSON, materialize a graph from
//! adapter-loaded rows, and evaluate allow/deny rules without depending on the
//! monorepo rebac-engine crate.

pub mod evaluator;
pub mod graph;
pub mod loader;
pub mod materialize;
pub mod subject;
pub mod types;

pub use evaluator::RebacEvaluator;
pub use graph::{
    field_value_as_string, field_values_equal, MemoryGraph, RebacGraphView, RelationshipLink,
    RowSnapshot,
};
pub use loader::{enforced_rules, parse_relationship_access_rules, rules_from_playbook, RebacLoadError};
pub use materialize::{
    binding_for_entity, build_memory_graph, entity_id_fields_from_bindings, link_columns_for_entity,
    GraphSnapshot,
};
pub use subject::{resolve_subject, SubjectResolveError};
pub use types::{
    FieldMatchRule, FieldMatchType, PathDirection, RebacAction, RebacEffect,
    RebacImplementationStatus, RelationshipAccessPathStep, RelationshipAccessRule,
    RelationshipAccessRules, RowKey, SubjectCondition, SubjectConditionOperator, SubjectContext,
};
