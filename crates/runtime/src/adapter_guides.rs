use adapter_core::AdapterAuthoringGuide;
use adapter_csv::authoring_guide as csv_guide;
use adapter_mongodb::authoring_guide as mongodb_guide;
use adapter_rest::authoring_guide as rest_guide;
use adapter_soql::authoring_guide as soql_guide;
use adapter_sql::{mssql_authoring_guide, mysql_authoring_guide, postgres_authoring_guide};

// Resolve static authoring guide for a profile adapter type string.
pub fn resolve_authoring_guide(adapter_type: &str) -> Option<AdapterAuthoringGuide> {
    match adapter_type.trim().to_ascii_lowercase().as_str() {
        "sql" => Some(postgres_authoring_guide()),
        "mysql" => Some(mysql_authoring_guide()),
        "mssql" => Some(mssql_authoring_guide()),
        "csv" => Some(csv_guide()),
        "soql" => Some(soql_guide()),
        "mongodb" | "mongo" => Some(mongodb_guide()),
        "rest" | "openapi" => Some(rest_guide()),
        _ => None,
    }
}

// Build MCP hint telling agents to call get_adapter_guide for a profile source id.
pub fn adapter_guide_next_step(source_id: &str) -> String {
    format!(
        "Call get_adapter_guide(source_id=\"{source_id}\") after list_sources, before propose_binding"
    )
}
