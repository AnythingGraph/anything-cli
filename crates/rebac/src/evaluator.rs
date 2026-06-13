use std::collections::{HashMap, HashSet};

use crate::graph::{field_value_as_string, field_values_equal, RebacGraphView};
use crate::types::{
    FieldMatchType, PathDirection, RebacAction, RebacEffect, RelationshipAccessPathStep,
    RelationshipAccessRule, RelationshipAccessRules, RowKey, SubjectConditionOperator,
    SubjectContext,
};

/// Evaluates playbook relationship access rules against a graph snapshot.
pub struct RebacEvaluator<'graph> {
    rules: RelationshipAccessRules,
    graph: &'graph dyn RebacGraphView,
}

impl<'graph> RebacEvaluator<'graph> {
    pub fn new(rules: RelationshipAccessRules, graph: &'graph dyn RebacGraphView) -> Self {
        Self { rules, graph }
    }

    pub fn rules(&self) -> &RelationshipAccessRules {
        &self.rules
    }

    /// True when a subject may perform an action on one resource row.
    pub fn row_allowed(
        &self,
        subject: &SubjectContext,
        action: RebacAction,
        resource_entity_name: &str,
        resource_row_id: &str,
    ) -> bool {
        self.allowed_row_ids(subject, action, resource_entity_name)
            .contains(resource_row_id)
    }

    /// Business identifiers a subject may access for one entity and action.
    pub fn allowed_row_ids(
        &self,
        subject: &SubjectContext,
        action: RebacAction,
        resource_entity_name: &str,
    ) -> HashSet<String> {
        let mut memoized = HashMap::new();
        self.allowed_row_ids_inner(subject, action, resource_entity_name, &mut memoized)
    }

    fn allowed_row_ids_inner(
        &self,
        subject: &SubjectContext,
        action: RebacAction,
        resource_entity_name: &str,
        memoized: &mut HashMap<(RebacAction, String), HashSet<String>>,
    ) -> HashSet<String> {
        let cache_key = (action, resource_entity_name.to_string());
        if let Some(cached) = memoized.get(&cache_key) {
            return cached.clone();
        }

        let mut allowed_row_ids = HashSet::new();
        let mut denied_row_ids = HashSet::new();

        for rule in &self.rules.rules {
            if rule.action != action || rule.resource_entity_name != resource_entity_name {
                continue;
            }
            if !subject_condition_matches(rule, subject, self.graph) {
                continue;
            }

            let matching_rows = self.rows_matching_rule(subject, rule, action, memoized);

            match rule.effect {
                RebacEffect::Allow => allowed_row_ids.extend(matching_rows),
                RebacEffect::Deny => denied_row_ids.extend(matching_rows),
            }
        }

        allowed_row_ids.retain(|row_id| !denied_row_ids.contains(row_id));

        if self.rules.deny_by_default {
            memoized.insert(cache_key, allowed_row_ids.clone());
            return allowed_row_ids;
        }

        if allowed_row_ids.is_empty() && denied_row_ids.is_empty() {
            let all_rows: HashSet<String> = self
                .graph
                .list_row_keys(resource_entity_name)
                .into_iter()
                .map(|key| key.row_id)
                .collect();
            memoized.insert(cache_key, all_rows.clone());
            return all_rows;
        }

        if allowed_row_ids.is_empty() {
            let mut all_rows: HashSet<String> = self
                .graph
                .list_row_keys(resource_entity_name)
                .into_iter()
                .map(|key| key.row_id)
                .collect();
            all_rows.retain(|row_id| !denied_row_ids.contains(row_id));
            memoized.insert(cache_key, all_rows.clone());
            return all_rows;
        }

        memoized.insert(cache_key, allowed_row_ids.clone());
        allowed_row_ids
    }

    fn rows_matching_rule(
        &self,
        subject: &SubjectContext,
        rule: &RelationshipAccessRule,
        action: RebacAction,
        memoized: &mut HashMap<(RebacAction, String), HashSet<String>>,
    ) -> HashSet<String> {
        let mut matched = HashSet::new();

        if let Some(field_match) = &rule.match_rule {
            if rule.path.is_some() {
                return matched;
            }
            for row_key in self.graph.list_row_keys(&rule.resource_entity_name) {
                if field_match_applies(
                    self.graph,
                    field_match,
                    &rule.resource_entity_name,
                    &row_key.row_id,
                    subject,
                ) {
                    matched.insert(row_key.row_id);
                }
            }
            return matched;
        }

        if let Some(path_steps) = &rule.path {
            let prior_allowed = rule
                .requires_prior_access_to
                .as_ref()
                .map(|entity_name| {
                    self.allowed_row_ids_inner(subject, action, entity_name, memoized)
                });

            for row_key in self.graph.list_row_keys(&rule.resource_entity_name) {
                if let Some(required_entity) = &rule.requires_prior_access_to {
                    let Some(prior_rows) = prior_allowed.as_ref() else {
                        continue;
                    };
                    if !path_matches_with_prior(
                        self.graph,
                        subject,
                        path_steps,
                        &rule.resource_entity_name,
                        &row_key.row_id,
                        required_entity,
                        prior_rows,
                    ) {
                        continue;
                    }
                } else if !path_matches(
                    self.graph,
                    subject,
                    path_steps,
                    &rule.resource_entity_name,
                    &row_key.row_id,
                ) {
                    continue;
                }
                matched.insert(row_key.row_id);
            }
            return matched;
        }

        matched
    }
}

fn subject_condition_matches(
    rule: &RelationshipAccessRule,
    subject: &SubjectContext,
    graph: &dyn RebacGraphView,
) -> bool {
    let Some(condition) = &rule.subject_condition else {
        return true;
    };

    let subject_key = RowKey {
        entity_name: subject.entity_name.clone(),
        row_id: subject.identifier_value.clone(),
    };
    let Some(snapshot) = graph.row_snapshot(&subject_key) else {
        return false;
    };

    let Some(field_value) = field_value_as_string(&snapshot.values, &condition.field) else {
        return false;
    };

    match condition.operator {
        SubjectConditionOperator::Equals => condition
            .values
            .first()
            .is_some_and(|expected| field_values_equal(field_value.as_str(), expected)),
        SubjectConditionOperator::In => condition
            .values
            .iter()
            .any(|expected| field_values_equal(field_value.as_str(), expected)),
    }
}

fn field_match_applies(
    graph: &dyn RebacGraphView,
    field_match: &crate::types::FieldMatchRule,
    resource_entity_name: &str,
    resource_row_id: &str,
    subject: &SubjectContext,
) -> bool {
    if field_match.match_type != FieldMatchType::FieldEqualsSubject {
        return false;
    }

    let resource_key = RowKey {
        entity_name: resource_entity_name.to_string(),
        row_id: resource_row_id.to_string(),
    };
    let subject_key = RowKey {
        entity_name: subject.entity_name.clone(),
        row_id: subject.identifier_value.clone(),
    };

    let Some(resource_snapshot) = graph.row_snapshot(&resource_key) else {
        return false;
    };
    let Some(subject_snapshot) = graph.row_snapshot(&subject_key) else {
        return false;
    };

    let resource_value =
        field_value_as_string(&resource_snapshot.values, &field_match.resource_field);
    let subject_value =
        field_value_as_string(&subject_snapshot.values, &field_match.subject_field);

    match (resource_value, subject_value) {
        (Some(left), Some(right)) => field_values_equal(&left, &right),
        _ => false,
    }
}

fn path_matches(
    graph: &dyn RebacGraphView,
    subject: &SubjectContext,
    path_steps: &[RelationshipAccessPathStep],
    resource_entity_name: &str,
    resource_row_id: &str,
) -> bool {
    if path_steps.is_empty() {
        return false;
    }

    let mut current_entity = subject.entity_name.clone();
    let mut current_row_ids = vec![subject.identifier_value.clone()];

    for step in path_steps {
        let mut next_row_ids = Vec::new();
        for current_row_id in &current_row_ids {
            let step_targets = match step.direction {
                PathDirection::Forward => graph.follow_forward(
                    &step.relationship_name,
                    &current_entity,
                    current_row_id,
                    &step.to_entity_name,
                ),
                PathDirection::Reverse => graph.follow_reverse(
                    &step.relationship_name,
                    &current_entity,
                    current_row_id,
                    &step.to_entity_name,
                ),
            };
            next_row_ids.extend(step_targets);
        }
        next_row_ids.sort();
        next_row_ids.dedup();
        if next_row_ids.is_empty() {
            return false;
        }
        current_entity = step.to_entity_name.clone();
        current_row_ids = next_row_ids;
    }

    current_entity == resource_entity_name && current_row_ids.iter().any(|row_id| row_id == resource_row_id)
}

fn path_matches_with_prior(
    graph: &dyn RebacGraphView,
    subject: &SubjectContext,
    path_steps: &[RelationshipAccessPathStep],
    resource_entity_name: &str,
    resource_row_id: &str,
    prior_entity_name: &str,
    prior_allowed_row_ids: &HashSet<String>,
) -> bool {
    if !path_matches(graph, subject, path_steps, resource_entity_name, resource_row_id) {
        return false;
    }

    for prior_row_id in prior_allowed_row_ids {
        let prior_key = RowKey {
            entity_name: prior_entity_name.to_string(),
            row_id: prior_row_id.clone(),
        };
        if graph.row_snapshot(&prior_key).is_some() {
            return true;
        }
    }

    prior_allowed_row_ids.contains(resource_row_id)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::*;
    use crate::graph::{MemoryGraph, RelationshipLink};
    use crate::types::{
        RebacEffect, RelationshipAccessPathStep, RelationshipAccessRule,
    };

    fn crm_payroll_rules() -> RelationshipAccessRules {
        RelationshipAccessRules {
            summary: "Users read own accounts and payroll".into(),
            subject_entity_name: "crm_user".into(),
            subject_identifier_field: "user_id".into(),
            deny_by_default: true,
            active: true,
            implementation_note: None,
            implementation_status: None,
            rules: vec![
                RelationshipAccessRule {
                    id: "own_accounts".into(),
                    name: "Own accounts".into(),
                    effect: RebacEffect::Allow,
                    action: RebacAction::Read,
                    resource_entity_name: "crm_account".into(),
                    description: "Accounts via owns_account".into(),
                    subject_condition: None,
                    path: Some(vec![RelationshipAccessPathStep {
                        relationship_name: "owns_account".into(),
                        direction: PathDirection::Forward,
                        from_entity_name: "crm_user".into(),
                        to_entity_name: "crm_account".into(),
                    }]),
                    match_rule: None,
                    requires_prior_access_to: None,
                },
                RelationshipAccessRule {
                    id: "own_payroll".into(),
                    name: "Own payroll".into(),
                    effect: RebacEffect::Allow,
                    action: RebacAction::Read,
                    resource_entity_name: "crm_payroll_record".into(),
                    description: "Payroll via user_has_payroll".into(),
                    subject_condition: None,
                    path: Some(vec![RelationshipAccessPathStep {
                        relationship_name: "user_has_payroll".into(),
                        direction: PathDirection::Forward,
                        from_entity_name: "crm_user".into(),
                        to_entity_name: "crm_payroll_record".into(),
                    }]),
                    match_rule: None,
                    requires_prior_access_to: None,
                },
            ],
        }
    }

  #[test]
    fn federated_crm_payroll_rebac_allows_linked_rows_only() {
        let graph = MemoryGraph::new()
            .upsert_row(
                "crm_user",
                "alex.ae",
                HashMap::from([
                    ("user_id".to_string(), json!("alex.ae")),
                    ("full_name".to_string(), json!("Alex Anderson")),
                ]),
            )
            .upsert_row(
                "crm_account",
                "Northwind Traders",
                HashMap::from([
                    ("account_name".to_string(), json!("Northwind Traders")),
                    ("owner_user_id".to_string(), json!("alex.ae")),
                ]),
            )
            .upsert_row(
                "crm_account",
                "Fabrikam Inc",
                HashMap::from([
                    ("account_name".to_string(), json!("Fabrikam Inc")),
                    ("owner_user_id".to_string(), json!("jordan.ae")),
                ]),
            )
            .upsert_row(
                "crm_payroll_record",
                "pay-001",
                HashMap::from([
                    ("payroll_id".to_string(), json!("pay-001")),
                    ("user_id".to_string(), json!("alex.ae")),
                ]),
            )
            .add_link(RelationshipLink {
                relationship_name: "owns_account".into(),
                subject_entity_name: "crm_user".into(),
                subject_row_id: "alex.ae".into(),
                object_entity_name: "crm_account".into(),
                object_row_id: "Northwind Traders".into(),
            })
            .add_link(RelationshipLink {
                relationship_name: "user_has_payroll".into(),
                subject_entity_name: "crm_user".into(),
                subject_row_id: "alex.ae".into(),
                object_entity_name: "crm_payroll_record".into(),
                object_row_id: "pay-001".into(),
            });

        let rules = crm_payroll_rules();
        let subject = SubjectContext {
            entity_name: "crm_user".into(),
            identifier_value: "alex.ae".into(),
        };
        let evaluator = RebacEvaluator::new(rules, &graph);

        let accounts = evaluator.allowed_row_ids(&subject, RebacAction::Read, "crm_account");
        assert!(accounts.contains("Northwind Traders"));
        assert!(!accounts.contains("Fabrikam Inc"));

        let payroll =
            evaluator.allowed_row_ids(&subject, RebacAction::Read, "crm_payroll_record");
        assert!(payroll.contains("pay-001"));
    }
}
