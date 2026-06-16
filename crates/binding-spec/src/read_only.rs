use crate::PlaybookBinding;

// Reject binding queries that could mutate or DDL-change connected data sources.
pub fn validate_read_only_binding_queries(binding: &PlaybookBinding) -> Vec<String> {
    let mut errors = Vec::new();

    for (entity_name, entity_binding) in &binding.entities {
        collect_query_errors(entity_name, "lookup", &entity_binding.lookup, &mut errors);
        collect_query_errors(entity_name, "operations", &entity_binding.operations, &mut errors);
    }

    for (relationship_name, relationship_binding) in &binding.relationships {
        collect_query_errors(
            relationship_name,
            "operations",
            &relationship_binding.operations,
            &mut errors,
        );
    }

    errors
}

// Check each query map entry and append validation errors.
fn collect_query_errors(
    binding_name: &str,
    section: &str,
    queries: &std::collections::HashMap<String, String>,
    errors: &mut Vec<String>,
) {
    for (operation_name, query_text) in queries {
        if let Some(reason) = read_only_query_violation(query_text) {
            errors.push(format!(
                "{binding_name}.{section}.{operation_name}: {reason}"
            ));
        }
    }
}

// Return an error message when query text is not read-only safe.
pub fn read_only_query_violation(query_text: &str) -> Option<String> {
    let trimmed = query_text.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("find:")
        || lower.starts_with("count:")
        || lower.starts_with("get ")
        || lower.starts_with("post ")
    {
        return None;
    }

    let upper = trimmed.to_uppercase();
    if !(upper.starts_with("SELECT") || upper.starts_with("WITH")) {
        return Some("only SELECT (or WITH … SELECT) queries are allowed".into());
    }

    for forbidden_keyword in [
        "INSERT", "UPDATE", "DELETE", "DROP", "ALTER", "TRUNCATE", "CREATE", "GRANT", "REVOKE",
        "MERGE", "EXEC", "EXECUTE", "CALL",
    ] {
        if contains_sql_keyword(&upper, forbidden_keyword) {
            return Some(format!("forbidden keyword '{forbidden_keyword}' in query"));
        }
    }

    None
}

// Match SQL keywords as standalone tokens (not inside identifiers).
fn contains_sql_keyword(upper_query: &str, keyword: &str) -> bool {
    upper_query
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|token| token == keyword)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn allows_select_queries() {
        assert!(read_only_query_violation("SELECT id FROM users WHERE id = :identifier").is_none());
    }

    #[test]
    fn rejects_insert_queries() {
        assert!(read_only_query_violation("INSERT INTO users VALUES (1)").is_some());
    }

    #[test]
    fn rejects_select_with_delete_subquery_keyword() {
        let mut operations = HashMap::new();
        operations.insert("bad".into(), "SELECT * FROM users; DELETE FROM users".into());
        let mut errors = Vec::new();
        collect_query_errors("crm_user", "operations", &operations, &mut errors);
        assert!(!errors.is_empty());
    }
}
