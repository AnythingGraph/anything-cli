use plan_ir::StepResult;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofEnvelope {
    pub ok: bool,
    pub playbook_id: String,
    #[serde(default)]
    pub subject_id: Option<String>,
    #[serde(default)]
    pub rebac_applied: bool,
    #[serde(default)]
    pub answer_text: Option<String>,
    pub steps: Vec<StepResult>,
    #[serde(default)]
    pub summary: Option<ProofSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofSummary {
    pub resolved_entity: Option<String>,
    pub resolved_id: Option<String>,
    pub count: Option<u64>,
    pub listed_rows: Option<usize>,
}

// Build a proof envelope from executed plan steps.
pub fn build_proof_envelope(
    playbook_id: String,
    subject_id: Option<String>,
    step_results: Vec<StepResult>,
    rebac_applied: bool,
) -> ProofEnvelope {
    let mut resolved_entity = None;
    let mut resolved_id = None;
    let mut count = None;
    let mut listed_rows = None;

    for step in &step_results {
        if let Some(entity_ref) = step.entity_ref.as_ref() {
            resolved_entity = Some(entity_ref.entity.clone());
            resolved_id = Some(entity_ref.id_value.clone());
        }
        if let Some(step_count) = step.count {
            count = Some(step_count);
        }
        if let Some(rows) = step.rows.as_ref() {
            listed_rows = Some(rows.len());
        }
    }

    let answer_text = build_answer_text(&resolved_entity, &resolved_id, count, listed_rows);

    ProofEnvelope {
        ok: true,
        playbook_id,
        subject_id,
        rebac_applied,
        answer_text,
        steps: step_results,
        summary: Some(ProofSummary {
            resolved_entity,
            resolved_id,
            count,
            listed_rows,
        }),
    }
}

// Format a short natural-language answer from proof summary fields.
fn build_answer_text(
    resolved_entity: &Option<String>,
    resolved_id: &Option<String>,
    count: Option<u64>,
    listed_rows: Option<usize>,
) -> Option<String> {
    if let Some(total) = count {
        let subject = resolved_id
            .as_ref()
            .map(|value| value.as_str())
            .unwrap_or("subject");
        return Some(format!("Count result for {subject}: {total}"));
    }
    if let Some(row_count) = listed_rows {
        return Some(format!("Listed {row_count} row(s)"));
    }
    if let (Some(entity), Some(id)) = (resolved_entity, resolved_id) {
        return Some(format!("Resolved {entity} id={id}"));
    }
    None
}
