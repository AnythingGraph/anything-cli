use reqwest::Client;
use serde_json::Value;

use crate::AdapterError;

const SALESFORCE_API_VERSION: &str = "v59.0";

// Return up to `limit` raw Salesforce records from one sObject (read-only discovery; no playbook).
pub async fn sample_salesforce_object(
    instance_url: &str,
    access_token: &str,
    object_name: &str,
    limit: u32,
) -> Result<(String, Vec<Value>), AdapterError> {
    validate_soql_object_name(object_name)?;

    let capped_limit = limit.max(1).min(100);
    let soql = format!("SELECT FIELDS(STANDARD) FROM {object_name} LIMIT {capped_limit}");
    let http_client = Client::new();
    let base_url = instance_url.trim_end_matches('/');
    let url = format!(
        "{base_url}/services/data/{SALESFORCE_API_VERSION}/query?q={}",
        urlencoding::encode(&soql)
    );

    let response = http_client
        .get(url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|error| AdapterError::Message(format!("soql sample request failed: {error}")))?;

    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AdapterError::Message(format!("soql sample error: {body}")));
    }

    let payload: Value = response
        .json()
        .await
        .map_err(|error| AdapterError::Message(format!("soql sample json parse failed: {error}")))?;

    let records = payload
        .get("records")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();

    Ok((soql, records))
}

// Reject object names that could enable SOQL injection.
fn validate_soql_object_name(object_name: &str) -> Result<(), AdapterError> {
    if object_name.is_empty() {
        return Err(AdapterError::Message("Salesforce object name is required".into()));
    }
    if !object_name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(AdapterError::Message(format!(
            "invalid Salesforce object name '{object_name}'"
        )));
    }
    Ok(())
}
