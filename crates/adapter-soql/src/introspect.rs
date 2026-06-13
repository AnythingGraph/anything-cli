use adapter_sql::{ColumnSchema, ForeignKeySchema, SourceSchemaCatalog, TableSchema};
use reqwest::Client;
use serde_json::Value;

use crate::AdapterError;

const SALESFORCE_API_VERSION: &str = "v59.0";

// Common CRM objects included when no filter is provided.
const DEFAULT_SOBJECT_NAMES: &[&str] = &[
    "User",
    "Lead",
    "Account",
    "Contact",
    "Opportunity",
    "Case",
    "Campaign",
    "Task",
    "Event",
];

// Read Salesforce object metadata for agent-driven binding workflows.
pub async fn introspect_salesforce_schema(
    instance_url: &str,
    access_token: &str,
    object_filter: Option<&str>,
) -> Result<SourceSchemaCatalog, AdapterError> {
    let http_client = Client::new();
    let base_url = instance_url.trim_end_matches('/');

    let object_names = resolve_object_names(&http_client, base_url, access_token, object_filter).await?;
    let mut tables = Vec::new();

    for object_name in object_names {
        let table_schema =
            describe_sobject(&http_client, base_url, access_token, &object_name).await?;
        tables.push(table_schema);
    }

    tables.sort_by(|left, right| left.table_name.cmp(&right.table_name));

    Ok(SourceSchemaCatalog {
        adapter: "soql".into(),
        schema_name: object_filter.unwrap_or("salesforce").to_string(),
        tables,
    })
}

// Decide which Salesforce objects to describe.
async fn resolve_object_names(
    http_client: &Client,
    base_url: &str,
    access_token: &str,
    object_filter: Option<&str>,
) -> Result<Vec<String>, AdapterError> {
    if let Some(filter_text) = object_filter.filter(|value| !value.trim().is_empty()) {
        let names: Vec<String> = filter_text
            .split(',')
            .map(|part| part.trim().to_string())
            .filter(|part| !part.is_empty())
            .collect();
        if !names.is_empty() {
            return Ok(names);
        }
    }

    let global_payload = salesforce_get(
        http_client,
        base_url,
        access_token,
        &format!("/services/data/{SALESFORCE_API_VERSION}/sobjects/"),
    )
    .await?;

    let mut object_names = Vec::new();
    if let Some(sobject_entries) = global_payload.get("sobjects").and_then(|value| value.as_array()) {
        for entry in sobject_entries {
            let name = entry
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            let queryable = entry
                .get("queryable")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            if !queryable || name.is_empty() {
                continue;
            }

            let is_default = DEFAULT_SOBJECT_NAMES
                .iter()
                .any(|default_name| default_name.eq_ignore_ascii_case(&name));
            let is_custom = name.ends_with("__c");

            if is_default || is_custom {
                object_names.push(name);
            }
        }
    }

    if object_names.is_empty() {
        object_names.extend(
            DEFAULT_SOBJECT_NAMES
                .iter()
                .map(|name| (*name).to_string()),
        );
    }

    object_names.sort();
    object_names.dedup();
    Ok(object_names)
}

// Describe one Salesforce object as a table schema catalog entry.
async fn describe_sobject(
    http_client: &Client,
    base_url: &str,
    access_token: &str,
    object_name: &str,
) -> Result<TableSchema, AdapterError> {
    let describe_payload = salesforce_get(
        http_client,
        base_url,
        access_token,
        &format!(
            "/services/data/{SALESFORCE_API_VERSION}/sobjects/{object_name}/describe"
        ),
    )
    .await?;

    let mut columns = Vec::new();
    let mut foreign_keys = Vec::new();

    if let Some(field_entries) = describe_payload.get("fields").and_then(|value| value.as_array()) {
        for field_entry in field_entries {
            let column_name = field_entry
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            if column_name.is_empty() {
                continue;
            }

            let data_type = field_entry
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown")
                .to_string();
            let is_nullable = field_entry
                .get("nillable")
                .and_then(|value| value.as_bool())
                .unwrap_or(true);

            columns.push(ColumnSchema {
                column_name: column_name.clone(),
                data_type,
                is_nullable,
            });

            if field_entry
                .get("type")
                .and_then(|value| value.as_str())
                == Some("reference")
            {
                if let Some(reference_targets) =
                    field_entry.get("referenceTo").and_then(|value| value.as_array())
                {
                    if let Some(target_object) =
                        reference_targets.first().and_then(|value| value.as_str())
                    {
                        foreign_keys.push(ForeignKeySchema {
                            column_name,
                            foreign_table_name: target_object.to_string(),
                            foreign_column_name: "Id".into(),
                        });
                    }
                }
            }
        }
    }

    Ok(TableSchema {
        table_name: object_name.to_string(),
        columns,
        foreign_keys,
    })
}

// Perform an authenticated Salesforce REST GET and parse JSON.
async fn salesforce_get(
    http_client: &Client,
    base_url: &str,
    access_token: &str,
    path: &str,
) -> Result<Value, AdapterError> {
    let url = format!("{base_url}{path}");
    let response = http_client
        .get(url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|error| AdapterError::Message(format!("salesforce request failed: {error}")))?;

    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AdapterError::Message(format!("salesforce error: {body}")));
    }

    response
        .json()
        .await
        .map_err(|error| AdapterError::Message(format!("salesforce json parse failed: {error}")))
}
