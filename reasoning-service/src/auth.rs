use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthRole {
    Public,
    User,
    Admin,
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub auth_required: bool,
    admin_tokens: HashSet<String>,
    user_tokens: HashSet<String>,
}

impl AuthConfig {
    // Load token-to-role mapping from environment variables.
    pub fn from_env() -> Self {
        let admin_tokens = parse_token_list(std::env::var("AG_ADMIN_TOKENS").ok());
        let user_tokens = parse_token_list(std::env::var("AG_USER_TOKENS").ok());
        let auth_required =
            !is_auth_disabled() && (!admin_tokens.is_empty() || !user_tokens.is_empty());

        Self {
            auth_required,
            admin_tokens,
            user_tokens,
        }
    }

    // Resolve caller role from bearer token; defaults to admin when auth is disabled.
    pub fn resolve_token(&self, bearer_token: Option<&str>) -> Result<AuthRole, AuthFailure> {
        if !self.auth_required {
            return Ok(AuthRole::Admin);
        }

        let Some(token) = bearer_token.filter(|value| !value.trim().is_empty()) else {
            return Err(AuthFailure::MissingToken);
        };

        if self.admin_tokens.contains(token) {
            return Ok(AuthRole::Admin);
        }
        if self.user_tokens.contains(token) {
            return Ok(AuthRole::User);
        }

        Err(AuthFailure::InvalidToken)
    }

    // Map HTTP route to minimum role required.
    pub fn required_role(method: &Method, path: &str) -> AuthRole {
        if method == Method::GET && path == "/health" {
            return AuthRole::Public;
        }

        if method == Method::GET && (path == "/playbooks" || path.ends_with("/context")) {
            return AuthRole::User;
        }

        if method == Method::POST
            && matches!(
                path,
                "/plan" | "/execute" | "/query" | "/rebac/allowed-rows"
            )
        {
            return AuthRole::User;
        }

        AuthRole::Admin
    }

    // True when caller role satisfies route requirement.
    pub fn role_allows(required: AuthRole, actual: AuthRole) -> bool {
        match required {
            AuthRole::Public => true,
            AuthRole::User => matches!(actual, AuthRole::User | AuthRole::Admin),
            AuthRole::Admin => actual == AuthRole::Admin,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AuthFailure {
    MissingToken,
    InvalidToken,
    Forbidden,
}

impl AuthFailure {
    pub fn status_code(self) -> StatusCode {
        match self {
            AuthFailure::MissingToken | AuthFailure::InvalidToken => StatusCode::UNAUTHORIZED,
            AuthFailure::Forbidden => StatusCode::FORBIDDEN,
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            AuthFailure::MissingToken => "missing Authorization: Bearer token",
            AuthFailure::InvalidToken => "invalid auth token",
            AuthFailure::Forbidden => "insufficient role for this endpoint",
        }
    }
}

impl IntoResponse for AuthFailure {
    fn into_response(self) -> Response {
        (self.status_code(), self.message()).into_response()
    }
}

// Parse comma-separated bearer tokens from env var text.
fn parse_token_list(raw_value: Option<String>) -> HashSet<String> {
    let mut tokens = HashSet::new();
    if let Some(raw_value) = raw_value {
        for token in raw_value.split(',') {
            let trimmed = token.trim();
            if !trimmed.is_empty() {
                tokens.insert(trimmed.to_string());
            }
        }
    }
    tokens
}

// True when AG_AUTH_DISABLED is set (local dev — no bearer token required).
fn is_auth_disabled() -> bool {
    match std::env::var("AG_AUTH_DISABLED")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("1") | Some("true") | Some("yes") => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_routes_allow_user_role() {
        assert!(AuthConfig::role_allows(
            AuthRole::User,
            AuthRole::User
        ));
        assert!(AuthConfig::role_allows(
            AuthRole::User,
            AuthRole::Admin
        ));
        assert!(!AuthConfig::role_allows(
            AuthRole::Admin,
            AuthRole::User
        ));
    }

    #[test]
    fn auth_disabled_env_skips_token_requirement() {
        std::env::set_var("AG_AUTH_DISABLED", "1");
        std::env::set_var("AG_ADMIN_TOKENS", "secret");
        let config = AuthConfig::from_env();
        assert!(!config.auth_required);
        assert_eq!(
            config.resolve_token(None).expect("admin when disabled"),
            AuthRole::Admin
        );
        std::env::remove_var("AG_AUTH_DISABLED");
        std::env::remove_var("AG_ADMIN_TOKENS");
    }
}
