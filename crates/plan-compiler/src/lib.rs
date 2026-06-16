use plan_ir::{
    CountRelationshipRequest, ListRelationshipRequest, Plan, PlanStep, QueryRequest,
    ResolveEntityRequest,
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

    let resolve_step = compile_resolve_step(playbook, &request.resolve)?;
    let mut steps = vec![resolve_step];

    if let Some(count_request) = request.count.as_ref() {
        steps.push(compile_count_step(count_request)?);
    } else if let Some(list_request) = request.list.as_ref() {
        steps.push(compile_list_step(list_request)?);
    }

    Ok(Plan {
        playbook_id: playbook.id.clone(),
        subject_id: request.subject_id.clone(),
        binding_name: request.binding_name.clone(),
        steps,
    })
}

// Compile resolve-entity step from request fragment and playbook entity metadata.
fn compile_resolve_step(
    playbook: &PlaybookDefinition,
    resolve: &ResolveEntityRequest,
) -> Result<PlanStep, CompileError> {
    let entity = playbook
        .entities
        .iter()
        .find(|entity| entity.name == resolve.entity)
        .ok_or_else(|| {
            CompileError::Validation(format!(
                "resolve entity '{}' is not defined in playbook '{}'",
                resolve.entity, playbook.id
            ))
        })?;

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
