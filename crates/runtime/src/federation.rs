use std::collections::HashMap;

use adapter_core::{build_exec_context, ExecutionState};
use anyhow::{anyhow, Result};
use binding_spec::PlaybookBinding;
use plan_ir::{Plan, PlanStep, StepResult};
use playbook_spec::PlaybookDefinition;
use rebac::binding_for_entity;

use crate::rebac_enforce;
use crate::ReasoningRuntime;

// Resolve which playbook entity a plan step reads from.
pub fn step_target_entity(
    playbook: &PlaybookDefinition,
    step: &PlanStep,
    state: &ExecutionState,
) -> Result<String> {
    match step {
        PlanStep::ResolveEntity { entity, .. } => Ok(entity.clone()),
        PlanStep::ListEntity { entity, .. } => Ok(entity.clone()),
        PlanStep::CountForSubject {
            relationship,
            object_entity,
        }
        | PlanStep::ListForSubject {
            relationship,
            object_entity,
            ..
        } => object_entity_for_relationship_step(playbook, relationship, object_entity, state),
    }
}

// Pick the binding that owns data for one plan step (federated routing).
pub fn resolve_binding_for_step(
    playbook: &PlaybookDefinition,
    bindings: &HashMap<String, PlaybookBinding>,
    plan: &Plan,
    step: &PlanStep,
    state: &ExecutionState,
) -> Result<PlaybookBinding> {
    let target_entity = step_target_entity(playbook, step, state)?;

    if let Some(binding_name) = plan.binding_name.as_deref() {
        if let Some(binding) = bindings.get(binding_name) {
            if binding.entities.contains_key(&target_entity) {
                return Ok(binding.clone());
            }
        }
    }

    binding_for_entity(playbook, bindings, &target_entity).ok_or_else(|| {
        anyhow!("no binding for playbook entity '{target_entity}'")
    })
}

// Execute a plan across multiple source bindings (one binding per step).
pub async fn execute_plan_federated(
    runtime: &ReasoningRuntime,
    playbook: &PlaybookDefinition,
    plan: &Plan,
    rebac_state: Option<&rebac_enforce::RebacState>,
) -> Result<Vec<StepResult>> {
    let bindings = runtime.bindings.read().await;
    let profile = runtime.profile.read().await.clone();

    let mut state = ExecutionState::default();
    let mut step_results = Vec::new();
    let mut access_subject: Option<rebac::SubjectContext> = None;

    if let Some(rebac_state) = rebac_state {
        if let Some(subject_id) = plan
            .subject_id
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            access_subject = Some(
                rebac::resolve_subject(&rebac_state.rules, &rebac_state.graph, subject_id)
                    .map_err(|error| anyhow!("rebac subject resolve failed: {error}"))?,
            );
        }
    }

    for (step_index, step) in plan.steps.iter().enumerate() {
        let binding = resolve_binding_for_step(playbook, &bindings, plan, step, &state)?;
        let context = build_exec_context(&binding, &profile)
            .map_err(|error| anyhow!("build exec context failed: {error}"))?;
        let adapter = runtime
            .adapters
            .get(&context.adapter_type)
            .ok_or_else(|| anyhow!("adapter not registered: {}", context.adapter_type))?;

        let step_result = adapter
            .execute_step(step_index, step, &binding, &context, &state)
            .await
            .map_err(|error| anyhow!("adapter step failed: {error}"))?;

        if let Some(entity_ref) = step_result.entity_ref.as_ref() {
            state.current_subject = Some(entity_ref.clone());
            if rebac_state.is_some() && access_subject.is_none() {
                access_subject = Some(rebac::SubjectContext {
                    entity_name: entity_ref.entity.clone(),
                    identifier_value: entity_ref.id_value.clone(),
                });
            }
        }

        let mut enforced_step_result = step_result;
        if let Some(rebac_state) = rebac_state {
            let subject = access_subject.as_ref().ok_or_else(|| {
                anyhow!("rebac enforced: provide subject_id or resolve the access subject entity")
            })?;
            rebac_enforce::apply_rebac_to_step(
                rebac_state,
                subject,
                step,
                &mut enforced_step_result,
            )
            .map_err(|error| anyhow!("{error}"))?;
        }

        step_results.push(enforced_step_result);
    }

    Ok(step_results)
}

// Infer the object entity for a relationship count/list step.
fn object_entity_for_relationship_step(
    playbook: &PlaybookDefinition,
    relationship_name: &str,
    object_entity: &Option<String>,
    state: &ExecutionState,
) -> Result<String> {
    if let Some(entity_name) = object_entity
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        return Ok(entity_name.to_string());
    }

    let subject_entity = state
        .current_subject
        .as_ref()
        .ok_or_else(|| anyhow!("relationship step requires a resolved subject entity"))?;

    let relationship = playbook
        .entity_relationships
        .iter()
        .find(|relationship| relationship.relationship_name == relationship_name)
        .ok_or_else(|| anyhow!("relationship '{relationship_name}' is not defined in playbook"))?;

    if relationship.subject_entity_name == subject_entity.entity {
        return Ok(relationship.object_entity_name.clone());
    }

    if relationship.object_entity_name == subject_entity.entity {
        return Ok(relationship.subject_entity_name.clone());
    }

    Err(anyhow!(
        "relationship '{relationship_name}' does not connect subject entity '{}'",
        subject_entity.entity
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use binding_spec::{EntityBinding, PlaybookBinding};
    use plan_ir::EntityRef;
    use playbook_spec::{PlaybookEntity, PlaybookEntityRelationship, PlaybookField};

    fn sample_playbook() -> PlaybookDefinition {
        PlaybookDefinition {
            id: "demo".into(),
            name: "Demo".into(),
            description: String::new(),
            category: String::new(),
            instructions: None,
            entities: vec![
                PlaybookEntity {
                    name: "customer".into(),
                    display_name: "Customer".into(),
                    fields: vec![
                        PlaybookField {
                            field_name: "user_id".into(),
                            field_type: String::new(),
                            is_identifier: true,
                        },
                        PlaybookField {
                            field_name: "name".into(),
                            field_type: String::new(),
                            is_identifier: false,
                        },
                    ],
                },
                PlaybookEntity {
                    name: "crm_user".into(),
                    display_name: "CRM user".into(),
                    fields: vec![PlaybookField {
                        field_name: "user_id".into(),
                        field_type: String::new(),
                        is_identifier: true,
                    }],
                },
            ],
            entity_relationships: vec![PlaybookEntityRelationship {
                relationship_name: "same_person".into(),
                subject_entity_name: "customer".into(),
                object_entity_name: "crm_user".into(),
                join_on: None,
            }],
            relationship_access_rules: None,
            entity_sources: Some(
                [
                    ("customer".into(), "mongodb".into()),
                    ("crm_user".into(), "postgres".into()),
                ]
                .into_iter()
                .collect(),
            ),
            bindings: Some(
                [
                    ("mongodb".into(), "demo.mongodb".into()),
                    ("postgres".into(), "demo.postgres".into()),
                ]
                .into_iter()
                .collect(),
            ),
            default_binding: None,
            field_mappings: None,
        }
    }

    #[test]
    fn routes_resolve_step_to_entity_binding() {
        let playbook = sample_playbook();
        let mut bindings = HashMap::new();
        bindings.insert(
            "demo.postgres".into(),
            PlaybookBinding {
                adapter: "sql".into(),
                version: 0,
                playbook_id: None,
                source_id: None,
                entities: HashMap::from([(
                    "crm_user".into(),
                    EntityBinding {
                        from: None,
                        id_field: "user_id".into(),
                        fields: HashMap::new(),
                        lookup: HashMap::new(),
                        operations: HashMap::new(),
                    },
                )]),
                relationships: HashMap::new(),
            },
        );
        bindings.insert(
            "demo.mongodb".into(),
            PlaybookBinding {
                adapter: "mongodb".into(),
                version: 0,
                playbook_id: None,
                source_id: None,
                entities: HashMap::from([(
                    "customer".into(),
                    EntityBinding {
                        from: None,
                        id_field: "user_id".into(),
                        fields: HashMap::new(),
                        lookup: HashMap::new(),
                        operations: HashMap::new(),
                    },
                )]),
                relationships: HashMap::new(),
            },
        );

        let plan = Plan {
            playbook_id: "demo".into(),
            subject_id: None,
            binding_name: Some("demo.postgres".into()),
            steps: vec![PlanStep::ResolveEntity {
                entity: "customer".into(),
                by_field: "name".into(),
                by_value: "Alex".into(),
            }],
        };

        let binding = resolve_binding_for_step(
            &playbook,
            &bindings,
            &plan,
            &plan.steps[0],
            &ExecutionState::default(),
        )
        .expect("customer resolve should route to mongodb binding");

        assert_eq!(binding.adapter, "mongodb");
    }

    #[test]
    fn routes_count_step_to_object_entity_binding() {
        let playbook = sample_playbook();
        let mut bindings = HashMap::new();
        bindings.insert(
            "demo.postgres".into(),
            PlaybookBinding {
                adapter: "sql".into(),
                version: 0,
                playbook_id: None,
                source_id: None,
                entities: HashMap::from([(
                    "crm_user".into(),
                    EntityBinding {
                        from: None,
                        id_field: "user_id".into(),
                        fields: HashMap::new(),
                        lookup: HashMap::new(),
                        operations: HashMap::new(),
                    },
                )]),
                relationships: HashMap::new(),
            },
        );
        bindings.insert(
            "demo.mongodb".into(),
            PlaybookBinding {
                adapter: "mongodb".into(),
                version: 0,
                playbook_id: None,
                source_id: None,
                entities: HashMap::from([(
                    "customer".into(),
                    EntityBinding {
                        from: None,
                        id_field: "user_id".into(),
                        fields: HashMap::new(),
                        lookup: HashMap::new(),
                        operations: HashMap::new(),
                    },
                )]),
                relationships: HashMap::new(),
            },
        );

        let plan = Plan {
            playbook_id: "demo".into(),
            subject_id: None,
            binding_name: Some("demo.mongodb".into()),
            steps: vec![PlanStep::CountForSubject {
                relationship: "same_person".into(),
                object_entity: None,
            }],
        };

        let state = ExecutionState {
            current_subject: Some(EntityRef {
                entity: "customer".into(),
                id_field: "user_id".into(),
                id_value: "alex.ae".into(),
                display_value: None,
            }),
        };

        let binding = resolve_binding_for_step(
            &playbook,
            &bindings,
            &plan,
            &plan.steps[0],
            &state,
        )
        .expect("count crm_user should route to postgres binding");

        assert_eq!(binding.adapter, "sql");
    }
}
