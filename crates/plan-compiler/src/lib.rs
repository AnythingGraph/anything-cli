use plan_ir::{
    CountRelationshipRequest, ListRelationshipRequest, Plan, PlanStep, QueryRequest,
    ResolveEntityRequest,
};
use playbook_spec::PlaybookDefinition;
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

    let resolve_step = compile_resolve_step(&request.resolve)?;
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

// Compile resolve-entity step from request fragment.
fn compile_resolve_step(resolve: &ResolveEntityRequest) -> Result<PlanStep, CompileError> {
    if let Some(name_value) = resolve.by_name.as_ref().filter(|value| !value.trim().is_empty()) {
        return Ok(PlanStep::ResolveEntity {
            entity: resolve.entity.clone(),
            by_field: "full_name".to_string(),
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
            by_field: "user_id".to_string(),
            by_value: identifier.trim().to_string(),
        });
    }
    Err(CompileError::Validation(
        "resolve requires by_name or by_identifier".into(),
    ))
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
