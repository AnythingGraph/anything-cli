use std::collections::HashMap;

use adapter_core::build_exec_context;
use anyhow::{anyhow, Result};
use binding_spec::PlaybookBinding;
use plan_ir::{PlanStep, StepResult};
use playbook_spec::PlaybookDefinition;
use rebac::{
    build_memory_graph, binding_for_entity, entity_id_fields_from_bindings, enforced_rules,
    rules_from_playbook, RebacAction, RebacEvaluator,
    RelationshipAccessRules, SubjectContext,
};
use rebac::GraphSnapshot;
use serde_json::Value;

use crate::ReasoningRuntime;

/// Cached ReBAC evaluation state for one playbook query.
pub struct RebacState {
    pub rules: RelationshipAccessRules,
    pub graph: rebac::MemoryGraph,
    pub entity_id_fields: HashMap<String, String>,
}

/// Build enforced ReBAC state for a playbook when rules are configured.
pub async fn try_build_rebac_state(
    runtime: &ReasoningRuntime,
    playbook: &PlaybookDefinition,
) -> Result<Option<RebacState>> {
    let rules = match rules_from_playbook(playbook)? {
        Some(rules) => enforced_rules(&rules),
        None => None,
    };

    if rules.is_none() {
        return Ok(None);
    }
    let rules = rules.expect("checked above");

    let snapshot = load_graph_snapshot(runtime, playbook).await?;
    let bindings_map = runtime.bindings.read().await;
    let collected_bindings = collect_playbook_bindings(playbook, &bindings_map);
    let binding_refs: Vec<&PlaybookBinding> = collected_bindings.iter().collect();
    let graph = build_memory_graph(playbook, &binding_refs, &snapshot);

    Ok(Some(RebacState {
        rules,
        graph,
        entity_id_fields: snapshot.entity_id_fields,
    }))
}

/// Apply ReBAC filtering to one executed step result.
pub fn apply_rebac_to_step(
    rebac_state: &RebacState,
    subject: &SubjectContext,
    plan_step: &PlanStep,
    step_result: &mut StepResult,
) -> Result<(), String> {
    let evaluator = RebacEvaluator::new(rebac_state.rules.clone(), &rebac_state.graph);

    match plan_step {
        PlanStep::ResolveEntity { entity, .. } => {
            if entity != &rebac_state.rules.subject_entity_name {
                return Err(format!(
                    "rebac enforced: resolve entity must be '{}', got '{entity}'",
                    rebac_state.rules.subject_entity_name
                ));
            }
            if let Some(entity_ref) = step_result.entity_ref.as_ref() {
                if let Some(expected_subject_id) = plan_subject_id_from_context(subject) {
                    if !rebac::field_values_equal(&entity_ref.id_value, &expected_subject_id) {
                        return Err(
                            "rebac enforced: resolved subject does not match subject_id".into(),
                        );
                    }
                }
            }
            Ok(())
        }
        PlanStep::CountForSubject {
            relationship,
            object_entity,
        } => {
            let resource_entity = object_entity
                .clone()
                .or_else(|| infer_object_entity_from_relationship(rebac_state, relationship))
                .unwrap_or_else(|| relationship.clone());

            let allowed_ids =
                evaluator.allowed_row_ids(subject, RebacAction::Read, &resource_entity);
            step_result.count = Some(allowed_ids.len() as u64);

            if let Some(rows) = step_result.rows.as_mut() {
                *rows = filter_rows_by_allowed_ids(
                    rows,
                    &resource_entity,
                    &allowed_ids,
                    rebac_state,
                );
            }

            Ok(())
        }
        PlanStep::ListForSubject {
            relationship,
            object_entity,
            ..
        } => {
            let resource_entity = object_entity
                .clone()
                .or_else(|| infer_object_entity_from_relationship(rebac_state, relationship))
                .unwrap_or_else(|| relationship.clone());

            let allowed_ids =
                evaluator.allowed_row_ids(subject, RebacAction::Read, &resource_entity);

            if let Some(rows) = step_result.rows.as_mut() {
                *rows = filter_rows_by_allowed_ids(
                    rows,
                    &resource_entity,
                    &allowed_ids,
                    rebac_state,
                );
            }

            step_result.count = Some(
                step_result
                    .rows
                    .as_ref()
                    .map(|rows| rows.len() as u64)
                    .unwrap_or(0),
            );
            Ok(())
        }
    }
}

/// List allowed row identifiers for one entity (HTTP / MCP discovery).
pub fn allowed_row_ids_for_entity(
    rebac_state: &RebacState,
    subject: &SubjectContext,
    entity_name: &str,
) -> Vec<String> {
    let evaluator = RebacEvaluator::new(rebac_state.rules.clone(), &rebac_state.graph);
    let mut allowed = evaluator
        .allowed_row_ids(subject, RebacAction::Read, entity_name)
        .into_iter()
        .collect::<Vec<_>>();
    allowed.sort();
    allowed
}

async fn load_graph_snapshot(
    runtime: &ReasoningRuntime,
    playbook: &PlaybookDefinition,
) -> Result<GraphSnapshot> {
    let bindings_map = runtime.bindings.read().await;
    let profile = runtime.profile.read().await.clone();
    let entity_id_fields = entity_id_fields_from_bindings(playbook, &bindings_map);

    let mut rows_by_entity: HashMap<String, Vec<HashMap<String, Value>>> = HashMap::new();

    for entity in &playbook.entities {
        let entity_name = entity.name.clone();
        let binding = binding_for_entity(playbook, &bindings_map, &entity_name)
            .ok_or_else(|| anyhow!("no binding for playbook entity '{entity_name}'"))?;

        let context = build_exec_context(&binding, &profile)
            .map_err(|error| anyhow!("build exec context failed: {error}"))?;
        let adapter = runtime
            .adapters
            .get(&context.adapter_type)
            .ok_or_else(|| anyhow!("adapter not registered: {}", context.adapter_type))?;

        let json_rows = adapter_core::DataAdapter::load_entity_rows(
            adapter.as_ref(),
            &entity_name,
            &binding,
            &context,
        )
            .await
            .map_err(|error| anyhow!("load entity rows failed for {entity_name}: {error}"))?;

        let id_field = entity_id_fields
            .get(&entity_name)
            .cloned()
            .unwrap_or_else(|| "id".to_string());

        let entity_rows = rows_by_entity.entry(entity_name).or_insert_with(Vec::new);
        merge_entity_rows(entity_rows, json_rows, &id_field);
    }

    Ok(GraphSnapshot {
        rows_by_entity,
        entity_id_fields,
    })
}

fn collect_playbook_bindings(
    playbook: &PlaybookDefinition,
    bindings_map: &HashMap<String, PlaybookBinding>,
) -> Vec<PlaybookBinding> {
    let mut collected = Vec::new();
    let mut seen_stems = Vec::new();

    for entity in &playbook.entities {
        if let Some(binding) = binding_for_entity(playbook, bindings_map, &entity.name) {
            let binding_stem = binding
                .source_id
                .clone()
                .unwrap_or_else(|| binding.adapter.clone());
            if !seen_stems.iter().any(|existing| existing == &binding_stem) {
                seen_stems.push(binding_stem);
                collected.push(binding);
            }
        }
    }

    if collected.is_empty() {
        for binding_name in bindings_map.keys() {
            if binding_name.starts_with(&playbook.id) {
                if let Some(binding) = bindings_map.get(binding_name) {
                    collected.push(binding.clone());
                }
            }
        }
    }

    collected
}

fn merge_entity_rows(
    existing_rows: &mut Vec<HashMap<String, Value>>,
    json_rows: Vec<Value>,
    id_field: &str,
) {
    for row_value in json_rows {
        if let Some(row_object) = row_value.as_object() {
            let row_map: HashMap<String, Value> =
                row_object.iter().map(|(key, value)| (key.clone(), value.clone())).collect();
            let row_id = row_map
                .get(id_field)
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());

            if let Some(row_id_value) = row_id {
                let already_present = existing_rows.iter().any(|existing| {
                    existing
                        .get(id_field)
                        .and_then(|value| value.as_str())
                        .map(|value| value.trim() == row_id_value)
                        .unwrap_or(false)
                });
                if !already_present {
                    existing_rows.push(row_map);
                }
            } else {
                existing_rows.push(row_map);
            }
        }
    }
}

fn filter_rows_by_allowed_ids(
    rows: &[Value],
    resource_entity: &str,
    allowed_ids: &std::collections::HashSet<String>,
    rebac_state: &RebacState,
) -> Vec<Value> {
    let id_field = rebac_state
        .entity_id_fields
        .get(resource_entity)
        .cloned()
        .unwrap_or_else(|| "id".to_string());

    rows.iter()
        .filter(|row| {
            row.as_object()
                .and_then(|object| object.get(&id_field))
                .and_then(|value| value.as_str())
                .map(|value| allowed_ids.contains(value.trim()))
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

fn infer_object_entity_from_relationship(
    rebac_state: &RebacState,
    relationship_name: &str,
) -> Option<String> {
    rebac_state
        .rules
        .rules
        .iter()
        .find(|rule| {
            rule.path.as_ref().is_some_and(|steps| {
                steps
                    .first()
                    .is_some_and(|step| step.relationship_name == relationship_name)
            })
        })
        .map(|rule| rule.resource_entity_name.clone())
}

fn plan_subject_id_from_context(subject: &SubjectContext) -> Option<String> {
    Some(subject.identifier_value.clone())
}
