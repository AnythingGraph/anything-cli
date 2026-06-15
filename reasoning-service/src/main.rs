use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use plan_ir::{Plan, QueryRequest};
use proof::ProofEnvelope;
use runtime::{
    default_paths_from_workspace, resolve_workspace_root, ReasoningRuntime, TestBindingRequest,
};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

mod auth;

use auth::{AuthConfig, AuthFailure, AuthRole};

#[derive(Clone)]
struct AppState {
    runtime: Arc<ReasoningRuntime>,
    auth: Arc<AuthConfig>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    ok: bool,
    service: String,
    playbooks: Vec<String>,
    auth_required: bool,
}

#[derive(Debug, Deserialize)]
struct ExecutePlanRequest {
    plan: Plan,
}

#[derive(Debug, Deserialize)]
struct IntrospectSourceRequest {
    #[serde(default)]
    schema_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProposeBindingRequest {
    binding_yaml: String,
}

#[derive(Debug, Deserialize)]
struct SaveBindingRequest {
    adapter_suffix: String,
    binding_yaml: String,
}

#[derive(Debug, Deserialize)]
struct ProposePlaybookRequest {
    playbook_json: String,
}

#[derive(Debug, Deserialize)]
struct SavePlaybookRequest {
    playbook_json: String,
}

#[derive(Debug, Deserialize)]
struct RebacAllowedRowsRequest {
    pub playbook_id: String,
    pub subject_id: String,
    #[serde(default)]
    pub entity_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SuggestBindingsRequest {
    source_id: String,
    #[serde(default)]
    schema_name: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let workspace_root = resolve_workspace_root();

    let mut config = default_paths_from_workspace(&workspace_root);
    if let Ok(playbooks_dir) = std::env::var("AG_PLAYBOOKS_DIR") {
        config.playbooks_dir = PathBuf::from(playbooks_dir);
    }
    if let Ok(bindings_dir) = std::env::var("AG_BINDINGS_DIR") {
        config.bindings_dir = PathBuf::from(bindings_dir);
    }
    if let Ok(profile_path) = std::env::var("AG_PROFILE_PATH") {
        config.profile_path = Some(PathBuf::from(profile_path));
    }

    let runtime = Arc::new(ReasoningRuntime::bootstrap(config).await?);
    let auth = Arc::new(AuthConfig::from_env());
    let state = AppState {
        runtime,
        auth: auth.clone(),
    };

    if auth.auth_required {
        tracing::info!("reasoning-service auth enabled (AG_ADMIN_TOKENS / AG_USER_TOKENS)");
    } else {
        tracing::warn!(
            "reasoning-service auth disabled — set AG_ADMIN_TOKENS and/or AG_USER_TOKENS for production"
        );
    }

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/sources", get(list_sources_handler))
        .route("/sources/{source_id}/introspect", post(introspect_source_handler))
        .route("/bindings", get(list_bindings_handler))
        .route("/bindings/{binding_name}", get(get_binding_handler))
        .route("/playbooks", get(list_playbooks_handler))
        .route("/playbooks/{playbook_id}/context", get(playbook_context_handler))
        .route(
            "/playbooks/{playbook_id}/propose-playbook",
            post(propose_playbook_handler),
        )
        .route(
            "/playbooks/{playbook_id}/save-playbook",
            post(save_playbook_handler),
        )
        .route(
            "/playbooks/{playbook_id}/suggest-bindings",
            post(suggest_bindings_handler),
        )
        .route(
            "/playbooks/{playbook_id}/propose-binding",
            post(propose_binding_handler),
        )
        .route(
            "/playbooks/{playbook_id}/test-binding",
            post(test_binding_handler),
        )
        .route(
            "/playbooks/{playbook_id}/save-binding",
            post(save_binding_handler),
        )
        .route("/plan", post(plan_handler))
        .route("/execute", post(execute_handler))
        .route("/query", post(query_handler))
        .route("/rebac/allowed-rows", post(rebac_allowed_rows_handler))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let host = std::env::var("AG_REASONING_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = std::env::var("AG_REASONING_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8787);
    let address: SocketAddr = format!("{host}:{port}").parse()?;
    let listener = TcpListener::bind(address).await?;
    tracing::info!("reasoning-service listening on http://{address}");
    axum::serve(listener, app).await?;
    Ok(())
}

// Enforce bearer token role checks before route handlers run.
async fn auth_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, AuthFailure> {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let required_role = AuthConfig::required_role(&method, &path);

    if required_role == AuthRole::Public {
        return Ok(next.run(request).await);
    }

    let bearer_token = extract_bearer_token(request.headers());
    let caller_role = state.auth.resolve_token(bearer_token.as_deref())?;

    if !AuthConfig::role_allows(required_role, caller_role) {
        return Err(AuthFailure::Forbidden);
    }

    Ok(next.run(request).await)
}

// Parse Authorization: Bearer <token> header value.
fn extract_bearer_token(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

async fn health_handler(State(state): State<AppState>) -> Json<HealthResponse> {
    let playbooks = state.runtime.list_playbook_ids().await;
    Json(HealthResponse {
        ok: true,
        service: "reasoning-service".into(),
        playbooks,
        auth_required: state.auth.auth_required,
    })
}

async fn list_playbooks_handler(State(state): State<AppState>) -> Json<Vec<String>> {
    Json(state.runtime.list_playbook_ids().await)
}

async fn playbook_context_handler(
    State(state): State<AppState>,
    Path(playbook_id): Path<String>,
) -> Result<Json<playbook_spec::PlaybookContextSummary>, (StatusCode, String)> {
    state
        .runtime
        .get_playbook_context(&playbook_id)
        .await
        .map(Json)
        .map_err(|error| (StatusCode::NOT_FOUND, error.to_string()))
}

async fn plan_handler(
    State(state): State<AppState>,
    Json(request): Json<QueryRequest>,
) -> Result<Json<Plan>, (StatusCode, String)> {
    state
        .runtime
        .compile_plan(&request)
        .await
        .map(Json)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))
}

async fn execute_handler(
    State(state): State<AppState>,
    Json(request): Json<ExecutePlanRequest>,
) -> Result<Json<ProofEnvelope>, (StatusCode, String)> {
    state
        .runtime
        .execute_plan(&request.plan)
        .await
        .map(Json)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))
}

async fn query_handler(
    State(state): State<AppState>,
    Json(request): Json<QueryRequest>,
) -> Result<Json<ProofEnvelope>, (StatusCode, String)> {
    state
        .runtime
        .query(&request)
        .await
        .map(Json)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))
}

async fn list_sources_handler(State(state): State<AppState>) -> Json<Vec<runtime::SourceSummary>> {
    Json(state.runtime.list_sources().await)
}

async fn list_bindings_handler(State(state): State<AppState>) -> Json<Vec<String>> {
    Json(state.runtime.list_bindings().await)
}

async fn get_binding_handler(
    State(state): State<AppState>,
    Path(binding_name): Path<String>,
) -> Result<Json<binding_spec::PlaybookBinding>, (StatusCode, String)> {
    state
        .runtime
        .get_binding(&binding_name)
        .await
        .map(Json)
        .map_err(|error| (StatusCode::NOT_FOUND, error.to_string()))
}

async fn introspect_source_handler(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
    Json(request): Json<IntrospectSourceRequest>,
) -> Result<Json<runtime::IntrospectSourceResponse>, (StatusCode, String)> {
    state
        .runtime
        .introspect_source(
            &source_id,
            request.schema_name.as_deref(),
        )
        .await
        .map(Json)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))
}

async fn suggest_bindings_handler(
    State(state): State<AppState>,
    Path(playbook_id): Path<String>,
    Json(request): Json<SuggestBindingsRequest>,
) -> Result<Json<runtime::SuggestBindingsResponse>, (StatusCode, String)> {
    state
        .runtime
        .suggest_bindings(
            &playbook_id,
            &request.source_id,
            request.schema_name.as_deref(),
        )
        .await
        .map(Json)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))
}

async fn propose_binding_handler(
    State(state): State<AppState>,
    Path(playbook_id): Path<String>,
    Json(request): Json<ProposeBindingRequest>,
) -> Result<Json<runtime::ProposeBindingResponse>, (StatusCode, String)> {
    state
        .runtime
        .propose_binding(&playbook_id, &request.binding_yaml)
        .await
        .map(Json)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))
}

async fn propose_playbook_handler(
    State(state): State<AppState>,
    Path(playbook_id): Path<String>,
    Json(request): Json<ProposePlaybookRequest>,
) -> Result<Json<runtime::ProposePlaybookResponse>, (StatusCode, String)> {
    state
        .runtime
        .propose_playbook(&playbook_id, &request.playbook_json)
        .await
        .map(Json)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))
}

async fn test_binding_handler(
    State(state): State<AppState>,
    Path(playbook_id): Path<String>,
    Json(mut request): Json<TestBindingRequest>,
) -> Result<Json<runtime::TestBindingResponse>, (StatusCode, String)> {
    request.playbook_id = playbook_id;
    state
        .runtime
        .test_binding(&request)
        .await
        .map(Json)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))
}

async fn rebac_allowed_rows_handler(
    State(state): State<AppState>,
    Json(request): Json<RebacAllowedRowsRequest>,
) -> Result<Json<runtime::RebacAllowedRowsResponse>, (StatusCode, String)> {
    state
        .runtime
        .rebac_allowed_rows(
            &request.playbook_id,
            &request.subject_id,
            request.entity_name.as_deref(),
        )
        .await
        .map(Json)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))
}

async fn save_binding_handler(
    State(state): State<AppState>,
    Path(playbook_id): Path<String>,
    Json(request): Json<SaveBindingRequest>,
) -> Result<Json<runtime::SaveBindingResponse>, (StatusCode, String)> {
    state
        .runtime
        .save_binding(&playbook_id, &request.adapter_suffix, &request.binding_yaml)
        .await
        .map(Json)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))
}

async fn save_playbook_handler(
    State(state): State<AppState>,
    Path(playbook_id): Path<String>,
    Json(request): Json<SavePlaybookRequest>,
) -> Result<Json<runtime::SavePlaybookResponse>, (StatusCode, String)> {
    state
        .runtime
        .save_playbook(&playbook_id, &request.playbook_json)
        .await
        .map(Json)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))
}
