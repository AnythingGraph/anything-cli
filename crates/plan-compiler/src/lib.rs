use plan_ir::{
    CountRelationshipRequest, ListEntityRequest, ListRelationshipRequest, Plan, PlanStep,
    QueryRequest, ResolveEntityRequest, SampleEntityRequest, DEFAULT_LIST_ENTITY_LIMIT,
    DEFAULT_SAMPLE_ENTITY_LIMIT,
};
use playbook_spec::{PlaybookDefinition, PlaybookEntity};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("validation error: {0}")]
    Validation(String),
}

// Compile a structured query request into a source-agnostic plan.
pub fn compile_query_request(
    playbook: &PlaybookDefinition,
    request: &QueryRequest,
) -> Result<Plan, CompileError> {
    if request.playbook_id != playbook.id {
        return Err(CompileError::Validation(format!(
            "playbook_id mismatch: expected {}, got {}",
            playbook.id, request.playbook_id
        )));
    }

    let mut steps = Vec::new();

    if let Some(list_request) = request.list_entity.as_ref() {
        steps.push(compile_list_entity_step(playbook, list_request, false)?);
    } else if let Some(sample_request) = request.sample_entity.as_ref() {
        steps.push(compile_sample_entity_step(playbook, sample_request)?);
    } else if let Some(resolve) = request.resolve.as_ref() {
        steps.push(compile_resolve_step(playbook, resolve)?);
        if let Some(count_request) = request.count.as_ref() {
            steps.push(compile_count_step(count_request)?);
        } else if let Some(list_request) = request.list.as_ref() {
            steps.push(compile_list_step(list_request)?);
        }
    } else {
        return Err(CompileError::Validation(
            "query requires resolve, list_entity, or sample_entity".into(),
        ));
    }

    Ok(Plan {
        playbook_id: playbook.id.clone(),
        subject_id: request.subject_id.clone(),
        binding_name: request.binding_name.clone(),
        steps,
    })
}

// Compile list-entity step from request fragment and playbook entity metadata.
fn compile_list_entity_step(
    playbook: &PlaybookDefinition,
    list: &ListEntityRequest,
    sample: bool,
) -> Result<PlanStep, CompileError> {
    validate_entity_name(playbook, &list.entity)?;
    let default_limit = if sample {
        DEFAULT_SAMPLE_ENTITY_LIMIT
    } else {
        DEFAULT_LIST_ENTITY_LIMIT
    };
    let limit = list.limit.unwrap_or(default_limit).max(1);
    Ok(PlanStep::ListEntity {
        entity: list.entity.clone(),
        limit,
        sample,
    })
}

// Compile sample-entity step (small list_entity with a lower default limit).
fn compile_sample_entity_step(
    playbook: &PlaybookDefinition,
    sample: &SampleEntityRequest,
) -> Result<PlanStep, CompileError> {
    compile_list_entity_step(
        playbook,
        &ListEntityRequest {
            entity: sample.entity.clone(),
            limit: sample.limit,
        },
        true,
    )
}

// Compile resolve-entity step from request fragment and playbook entity metadata.
fn compile_resolve_step(
    playbook: &PlaybookDefinition,
    resolve: &ResolveEntityRequest,
) -> Result<PlanStep, CompileError> {
    let entity = validate_entity_name(playbook, &resolve.entity)?;

    if let Some(name_value) = resolve.by_name.as_ref().filter(|value| !value.trim().is_empty()) {
        return Ok(PlanStep::ResolveEntity {
            entity: resolve.entity.clone(),
            by_field: resolve_name_field_for_entity(entity),
            by_value: name_value.trim().to_string(),
        });
    }
    if let Some(identifier) = resolve
        .by_identifier
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(PlanStep::ResolveEntity {
            entity: resolve.entity.clone(),
            by_field: resolve_identifier_field_for_entity(entity),
            by_value: identifier.trim().to_string(),
        });
    }
    Err(CompileError::Validation(
        "resolve requires by_name or by_identifier".into(),
    ))
}

// Ensure an entity name exists on the playbook.
fn validate_entity_name<'playbook>(
    playbook: &'playbook PlaybookDefinition,
    entity_name: &str,
) -> Result<&'playbook PlaybookEntity, CompileError> {
    playbook
        .entities
        .iter()
        .find(|entity| entity.name == entity_name)
        .ok_or_else(|| {
            CompileError::Validation(format!(
                "entity '{entity_name}' is not defined in playbook '{}'",
                playbook.id
            ))
        })
}

// Pick the playbook attribute used for display-name resolution.
fn resolve_name_field_for_entity(entity: &PlaybookEntity) -> String {
    let preferred_keys = [
        "full_name",
        "name",
        "legal_name",
        "display_name",
        "title",
    ];
    for preferred_key in preferred_keys {
        if entity
            .fields
            .iter()
            .any(|field| field.field_name == preferred_key && !field.is_identifier)
        {
            return preferred_key.to_string();
        }
    }

    entity
        .fields
        .iter()
        .find(|field| !field.is_identifier)
        .map(|field| field.field_name.clone())
        .unwrap_or_else(|| "name".to_string())
}

// Pick the playbook identifier field for id-based resolution.
fn resolve_identifier_field_for_entity(entity: &PlaybookEntity) -> String {
    entity
        .fields
        .iter()
        .find(|field| field.is_identifier)
        .map(|field| field.field_name.clone())
        .unwrap_or_else(|| "id".to_string())
}

// Compile count-for-subject step from request fragment.
fn compile_count_step(count: &CountRelationshipRequest) -> Result<PlanStep, CompileError> {
    Ok(PlanStep::CountForSubject {
        relationship: count.relationship.clone(),
        object_entity: count.object_entity.clone(),
    })
}

// Compile list-for-subject step from request fragment.
fn compile_list_step(list: &ListRelationshipRequest) -> Result<PlanStep, CompileError> {
    Ok(PlanStep::ListForSubject {
        relationship: list.relationship.clone(),
        object_entity: list.object_entity.clone(),
        limit: list.limit.unwrap_or(25),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use plan_ir::QueryRequest;
    use playbook_spec::{PlaybookEntity, PlaybookField};

    fn sample_playbook() -> PlaybookDefinition {
        PlaybookDefinition {
            id: "demo".into(),
            name: "Demo".into(),
            description: String::new(),
            category: String::new(),
            instructions: None,
            entities: vec![PlaybookEntity {
                name: "crm_user".into(),
                display_name: "CRM user".into(),
                fields: vec![PlaybookField {
                    field_name: "user_id".into(),
                    field_type: String::new(),
                    is_identifier: true,
                }],
            }],
            entity_relationships: vec![],
            relationship_access_rules: None,
            entity_sources: None,
            bindings: None,
            default_binding: None,
            field_mappings: None,
        }
    }

    #[test]
    fn compiles_list_entity_without_resolve() {
        let playbook = sample_playbook();
        let request = QueryRequest {
            playbook_id: "demo".into(),
            subject_id: None,
            binding_name: None,
            resolve: None,
            list_entity: Some(ListEntityRequest {
                entity: "crm_user".into(),
                limit: Some(50),
            }),
            sample_entity: None,
            count: None,
            list: None,
        };

        let plan = compile_query_request(&playbook, &request).expect("compile list entity");
        assert_eq!(plan.steps.len(), 1);
        match &plan.steps[0] {
            PlanStep::ListEntity {
                entity,
                limit,
                sample,
            } => {
                assert_eq!(entity, "crm_user");
                assert_eq!(*limit, 50);
                assert!(!*sample);
            }
            other => panic!("expected list_entity, got {other:?}"),
        }
    }

    #[test]
    fn compiles_sample_entity_with_default_limit() {
        let playbook = sample_playbook();
        let request = QueryRequest {
            playbook_id: "demo".into(),
            subject_id: None,
            binding_name: None,
            resolve: None,
            list_entity: None,
            sample_entity: Some(SampleEntityRequest {
                entity: "crm_user".into(),
                limit: None,
            }),
            count: None,
            list: None,
        };

        let plan = compile_query_request(&playbook, &request).expect("compile sample entity");
        match &plan.steps[0] {
            PlanStep::ListEntity { limit, sample, .. } => {
                assert_eq!(*limit, DEFAULT_SAMPLE_ENTITY_LIMIT);
                assert!(*sample);
            }
            other => panic!("expected list_entity sample, got {other:?}"),
        }
    }
}
