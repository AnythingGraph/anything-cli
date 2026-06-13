use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use adapter_core::{build_exec_context, AdapterRegistry, DataAdapter, ExecutionState};
use adapter_csv::{introspect_csv_file, CsvAdapter};
use adapter_mongodb::{introspect_mongodb_schema, MongoDbAdapter};
use adapter_rest::{introspect_rest_schema, RestAdapter};
use adapter_soql::{introspect_salesforce_schema, SoqlAdapter};
use adapter_sql::{
    introspect_mssql_schema, introspect_mysql_schema, introspect_postgres_schema, MssqlAdapter,
    MysqlAdapter, SqlAdapter, SourceSchemaCatalog,
};
use anyhow::{anyhow, Context, Result};
use binding_spec::{
    compile_binding_queries, load_binding_from_path, load_binding_from_yaml, load_profile_from_path,
    playbook_binding_path, playbook_binding_stem, save_binding_to_path,
    suggest_entity_table_mappings, validate_binding_for_playbook, BindingValidationReport,
    PlaybookBinding, SourceProfile,
};
use serde::{Deserialize, Serialize};
use plan_compiler::compile_query_request;
use plan_ir::{Plan, QueryRequest};
use playbook_spec::{
    discover_playbooks_in_directory, load_playbook_from_path, playbook_context_summary,
    resolve_binding_name_for_entity, PlaybookContextSummary, PlaybookDefinition,
};
use proof::ProofEnvelope;
use tokio::sync::RwLock;

mod rebac_enforce;

pub struct RuntimeConfig {
    pub playbooks_dir: PathBuf,
    pub bindings_dir: PathBuf,
    pub profile_path: Option<PathBuf>,
}

pub struct ReasoningRuntime {
    config: RuntimeConfig,
    pub(crate) playbooks: RwLock<HashMap<String, PlaybookDefinition>>,
    pub(crate) bindings: RwLock<HashMap<String, PlaybookBinding>>,
    pub(crate) profile: RwLock<SourceProfile>,
    pub(crate) adapters: AdapterRegistry,
}

impl ReasoningRuntime {
    // Create runtime and load playbooks, bindings, and profile from disk.
    pub async fn bootstrap(config: RuntimeConfig) -> Result<Self> {
        let mut runtime = Self {
            config,
            playbooks: RwLock::new(HashMap::new()),
            bindings: RwLock::new(HashMap::new()),
            profile: RwLock::new(SourceProfile {
                sources: HashMap::new(),
            }),
            adapters: AdapterRegistry::new(),
        };

        runtime
            .adapters
            .register(Arc::new(SqlAdapter) as Arc<dyn DataAdapter>);
        runtime
            .adapters
            .register(Arc::new(MysqlAdapter) as Arc<dyn DataAdapter>);
        runtime
            .adapters
            .register(Arc::new(MssqlAdapter) as Arc<dyn DataAdapter>);
        runtime
            .adapters
            .register(Arc::new(SoqlAdapter::default()) as Arc<dyn DataAdapter>);
        runtime
            .adapters
            .register(Arc::new(CsvAdapter) as Arc<dyn DataAdapter>);
        runtime
            .adapters
            .register(Arc::new(MongoDbAdapter) as Arc<dyn DataAdapter>);
        runtime
            .adapters
            .register(Arc::new(RestAdapter) as Arc<dyn DataAdapter>);

        runtime.reload_catalog().await?;
        Ok(runtime)
    }

    // Reload playbooks, bindings, and profile from configured directories.
    pub async fn reload_catalog(&self) -> Result<()> {
        let mut playbooks = HashMap::new();
        for playbook_path in discover_playbooks_in_directory(&self.config.playbooks_dir)? {
            let playbook = load_playbook_from_path(&playbook_path)?;
            playbooks.insert(playbook.id.clone(), playbook);
        }

        let mut bindings = HashMap::new();
        if self.config.bindings_dir.exists() {
            for entry in std::fs::read_dir(&self.config.bindings_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) == Some("yaml")
                    || path.extension().and_then(|ext| ext.to_str()) == Some("yml")
                {
                    let binding = load_binding_from_path(&path)?;
                    let binding_name = path
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or("binding")
                        .to_string();
                    bindings.insert(binding_name, binding);
                }
            }
        }

        let mut profile = if let Some(profile_path) = self.config.profile_path.as_ref() {
            load_profile_from_path(profile_path)?
        } else {
            SourceProfile {
                sources: HashMap::new(),
            }
        };
        resolve_profile_env_refs(&mut profile);

        *self.playbooks.write().await = playbooks;
        *self.bindings.write().await = bindings;
        *self.profile.write().await = profile;
        Ok(())
    }

    // List loaded playbook ids.
    pub async fn list_playbook_ids(&self) -> Vec<String> {
        let playbooks = self.playbooks.read().await;
        let mut ids: Vec<String> = playbooks.keys().cloned().collect();
        ids.sort();
        ids
    }

    // Return playbook context summary for agents.
    pub async fn get_playbook_context(&self, playbook_id: &str) -> Result<PlaybookContextSummary> {
        let playbooks = self.playbooks.read().await;
        let playbook = playbooks
            .get(playbook_id)
            .ok_or_else(|| anyhow!("playbook not found: {playbook_id}"))?;
        Ok(playbook_context_summary(playbook))
    }

    // Compile a query request into a plan.
    pub async fn compile_plan(&self, request: &QueryRequest) -> Result<Plan> {
        let playbooks = self.playbooks.read().await;
        let playbook = playbooks
            .get(&request.playbook_id)
            .ok_or_else(|| anyhow!("playbook not found: {}", request.playbook_id))?;
        let resolved_request = self.apply_binding_routing(playbook, request);
        compile_query_request(playbook, &resolved_request).context("compile plan failed")
    }

    // Execute a plan against configured bindings and adapters.
    pub async fn execute_plan(&self, plan: &Plan) -> Result<ProofEnvelope> {
        let playbooks = self.playbooks.read().await;
        let playbook = playbooks
            .get(&plan.playbook_id)
            .ok_or_else(|| anyhow!("playbook not found: {}", plan.playbook_id))?
            .clone();
        drop(playbooks);

        let rebac_state = rebac_enforce::try_build_rebac_state(self, &playbook).await?;
        let binding = self.resolve_binding(plan).await?;
        self.execute_plan_with_binding_and_rebac(plan, &binding, rebac_state.as_ref())
            .await
    }

    // Execute a plan using an explicit binding (used for binding tests).
    pub async fn execute_plan_with_binding(
        &self,
        plan: &Plan,
        binding: &PlaybookBinding,
    ) -> Result<ProofEnvelope> {
        let playbooks = self.playbooks.read().await;
        let playbook = playbooks
            .get(&plan.playbook_id)
            .ok_or_else(|| anyhow!("playbook not found: {}", plan.playbook_id))?
            .clone();
        drop(playbooks);

        let rebac_state = rebac_enforce::try_build_rebac_state(self, &playbook).await?;
        self.execute_plan_with_binding_and_rebac(plan, binding, rebac_state.as_ref())
            .await
    }

    // Execute plan with optional ReBAC enforcement state.
    async fn execute_plan_with_binding_and_rebac(
        &self,
        plan: &Plan,
        binding: &PlaybookBinding,
        rebac_state: Option<&rebac_enforce::RebacState>,
    ) -> Result<ProofEnvelope> {
        let profile = self.profile.read().await.clone();
        let context = build_exec_context(binding, &profile)
            .map_err(|error| anyhow!("build exec context failed: {error}"))?;

        let adapter = self
            .adapters
            .get(&context.adapter_type)
            .ok_or_else(|| anyhow!("adapter not registered: {}", context.adapter_type))?;

        let mut state = ExecutionState::default();
        let mut step_results = Vec::new();
        let mut access_subject: Option<rebac::SubjectContext> = None;

        if let Some(rebac_state) = rebac_state {
            if let Some(subject_id) = plan
                .subject_id
                .as_ref()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
            {
                access_subject = Some(
                    rebac::resolve_subject(&rebac_state.rules, &rebac_state.graph, subject_id)
                        .map_err(|error| anyhow!("rebac subject resolve failed: {error}"))?,
                );
            }
        }

        for (step_index, step) in plan.steps.iter().enumerate() {
            let step_result = adapter
                .execute_step(step_index, step, binding, &context, &state)
                .await
                .map_err(|error| anyhow!("adapter step failed: {error}"))?;

            if let Some(entity_ref) = step_result.entity_ref.as_ref() {
                state.current_subject = Some(entity_ref.clone());
                if rebac_state.is_some() && access_subject.is_none() {
                    access_subject = Some(rebac::SubjectContext {
                        entity_name: entity_ref.entity.clone(),
                        identifier_value: entity_ref.id_value.clone(),
                    });
                }
            }

            let mut enforced_step_result = step_result;
            if let Some(rebac_state) = rebac_state {
                let subject = access_subject.as_ref().ok_or_else(|| {
                    anyhow!("rebac enforced: provide subject_id or resolve the access subject entity")
                })?;
                rebac_enforce::apply_rebac_to_step(
                    rebac_state,
                    subject,
                    step,
                    &mut enforced_step_result,
                )
                .map_err(|error| anyhow!("{error}"))?;
            }

            step_results.push(enforced_step_result);
        }

        let rebac_applied = rebac_state.is_some();
        Ok(proof::build_proof_envelope(
            plan.playbook_id.clone(),
            plan.subject_id.clone(),
            step_results,
            rebac_applied,
        ))
    }

    // List row ids a subject may read for one entity under enforced ReBAC.
    pub async fn rebac_allowed_rows(
        &self,
        playbook_id: &str,
        subject_id: &str,
        entity_name: Option<&str>,
    ) -> Result<RebacAllowedRowsResponse> {
        let playbooks = self.playbooks.read().await;
        let playbook = playbooks
            .get(playbook_id)
            .ok_or_else(|| anyhow!("playbook not found: {playbook_id}"))?
            .clone();
        drop(playbooks);

        let rebac_state = rebac_enforce::try_build_rebac_state(self, &playbook)
            .await?
            .ok_or_else(|| anyhow!("playbook '{playbook_id}' has no enforced relationship_access_rules"))?;

        let subject = rebac::resolve_subject(&rebac_state.rules, &rebac_state.graph, subject_id)
            .map_err(|error| anyhow!("rebac subject resolve failed: {error}"))?;

        let entity_names: Vec<String> = if let Some(entity) = entity_name {
            vec![entity.to_string()]
        } else {
            playbook.entities.iter().map(|entity| entity.name.clone()).collect()
        };

        let mut allowed_by_entity = HashMap::new();
        for entity in entity_names {
            let allowed_ids =
                rebac_enforce::allowed_row_ids_for_entity(&rebac_state, &subject, &entity);
            allowed_by_entity.insert(entity, allowed_ids);
        }

        Ok(RebacAllowedRowsResponse {
            playbook_id: playbook_id.to_string(),
            subject_id: subject_id.to_string(),
            rebac_applied: true,
            allowed_rows_by_entity: allowed_by_entity,
        })
    }

    // Compile and execute in one call.
    pub async fn query(&self, request: &QueryRequest) -> Result<ProofEnvelope> {
        let plan = self.compile_plan(request).await?;
        self.execute_plan(&plan).await
    }

    // Pick binding_name from playbook entity_sources + bindings when omitted on the request.
    fn apply_binding_routing(
        &self,
        playbook: &PlaybookDefinition,
        request: &QueryRequest,
    ) -> QueryRequest {
        if request.binding_name.is_some() {
            return request.clone();
        }

        let routing_entity = infer_routing_entity(playbook, request);
        let mut resolved_request = request.clone();
        if let Some(binding_name) = resolve_binding_name_for_entity(playbook, &routing_entity) {
            resolved_request.binding_name = Some(binding_name);
        }
        resolved_request
    }

    // Validate all loaded playbooks.
    pub async fn validate_all_playbooks(&self) -> Result<()> {
        let playbooks = self.playbooks.read().await;
        if playbooks.is_empty() {
            return Err(anyhow!("no playbooks loaded from {:?}", self.config.playbooks_dir));
        }
        Ok(())
    }

    // List configured source ids from the active profile.
    pub async fn list_sources(&self) -> Vec<SourceSummary> {
        let profile = self.profile.read().await;
        let mut sources = Vec::new();
        for (source_id, connection) in &profile.sources {
            sources.push(SourceSummary {
                source_id: source_id.clone(),
                adapter: connection.adapter.clone(),
                has_dsn: connection.dsn.as_ref().is_some_and(|value| !value.trim().is_empty()),
                has_instance_url: connection
                    .instance_url
                    .as_ref()
                    .is_some_and(|value| !value.trim().is_empty()),
            });
        }
        sources.sort_by(|left, right| left.source_id.cmp(&right.source_id));
        sources
    }

    // List loaded binding file stems.
    pub async fn list_bindings(&self) -> Vec<String> {
        let bindings = self.bindings.read().await;
        let mut names: Vec<String> = bindings.keys().cloned().collect();
        names.sort();
        names
    }

    // Return one loaded binding by stem name.
    pub async fn get_binding(&self, binding_name: &str) -> Result<PlaybookBinding> {
        let bindings = self.bindings.read().await;
        bindings
            .get(binding_name)
            .cloned()
            .ok_or_else(|| anyhow!("binding not found: {binding_name}"))
    }

    // Introspect a configured source for agent-driven binding generation.
    pub async fn introspect_source(
        &self,
        source_id: &str,
        schema_name: Option<&str>,
    ) -> Result<IntrospectSourceResponse> {
        let profile = self.profile.read().await;
        let connection = profile
            .sources
            .get(source_id)
            .ok_or_else(|| anyhow!("profile missing source id: {source_id}"))?;

        let schema = match connection.adapter.as_str() {
            "sql" => {
                let dsn = connection
                    .dsn
                    .as_ref()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| anyhow!("source '{source_id}' is missing dsn"))?;
                let catalog = introspect_postgres_schema(dsn, schema_name)
                    .await
                    .map_err(|error| anyhow!("postgres introspection failed: {error}"))?;
                Some(serde_json::to_value(catalog)?)
            }
            "soql" => {
                let instance_url = connection
                    .instance_url
                    .as_ref()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| anyhow!("source '{source_id}' is missing instance_url"))?;
                let access_token = connection
                    .auth
                    .as_ref()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| anyhow!("source '{source_id}' is missing auth token"))?;
                let catalog = introspect_salesforce_schema(instance_url, access_token, schema_name)
                    .await
                    .map_err(|error| anyhow!("salesforce introspection failed: {error}"))?;
                Some(serde_json::to_value(catalog)?)
            }
            "csv" => {
                let file_path = connection
                    .file_path
                    .as_ref()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| anyhow!("source '{source_id}' is missing file_path"))?;
                let catalog = introspect_csv_file(Path::new(file_path))
                    .map_err(|error| anyhow!("csv introspection failed: {error}"))?;
                Some(serde_json::to_value(catalog)?)
            }
            "mysql" => {
                let dsn = connection
                    .dsn
                    .as_ref()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| anyhow!("source '{source_id}' is missing dsn"))?;
                let catalog = introspect_mysql_schema(dsn, schema_name)
                    .await
                    .map_err(|error| anyhow!("mysql introspection failed: {error}"))?;
                Some(serde_json::to_value(catalog)?)
            }
            "mssql" => {
                let dsn = connection
                    .dsn
                    .as_ref()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| anyhow!("source '{source_id}' is missing dsn"))?;
                let catalog = introspect_mssql_schema(dsn, schema_name)
                    .await
                    .map_err(|error| anyhow!("mssql introspection failed: {error}"))?;
                Some(serde_json::to_value(catalog)?)
            }
            "mongodb" => {
                let dsn = connection
                    .dsn
                    .as_ref()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| anyhow!("source '{source_id}' is missing dsn"))?;
                let catalog = introspect_mongodb_schema(
                    dsn,
                    connection.database.as_deref().or(schema_name),
                )
                .await
                .map_err(|error| anyhow!("mongodb introspection failed: {error}"))?;
                Some(serde_json::to_value(catalog)?)
            }
            "rest" => {
                let base_url = connection
                    .base_url
                    .as_ref()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| anyhow!("source '{source_id}' is missing base_url"))?;
                let catalog = introspect_rest_schema(base_url, schema_name);
                Some(serde_json::to_value(catalog)?)
            }
            other => return Err(anyhow!("introspection not supported for adapter: {other}")),
        };

        Ok(IntrospectSourceResponse {
            source_id: source_id.to_string(),
            adapter: connection.adapter.clone(),
            schema,
            message: None,
        })
    }

    // Validate a proposed binding YAML against a playbook without saving.
    pub async fn propose_binding(
        &self,
        playbook_id: &str,
        binding_yaml: &str,
    ) -> Result<ProposeBindingResponse> {
        let playbooks = self.playbooks.read().await;
        let playbook = playbooks
            .get(playbook_id)
            .ok_or_else(|| anyhow!("playbook not found: {playbook_id}"))?;

        let mut binding = load_binding_from_yaml(binding_yaml)
            .map_err(|error| anyhow!("invalid binding yaml: {error}"))?;
        if binding.playbook_id.is_none() {
            binding.playbook_id = Some(playbook_id.to_string());
        }
        compile_binding_queries(&mut binding);

        let validation = validate_binding_for_playbook(playbook, &binding);
        let compiled_yaml = binding_spec::binding_to_yaml(&binding)
            .map_err(|error| anyhow!("serialize binding failed: {error}"))?;

        Ok(ProposeBindingResponse {
            playbook_id: playbook_id.to_string(),
            binding_name: default_binding_name_for_playbook(playbook, &binding),
            validation,
            compiled_binding_yaml: compiled_yaml,
            binding,
        })
    }

    // Test a binding by compiling and optionally executing a sample query.
    pub async fn test_binding(&self, request: &TestBindingRequest) -> Result<TestBindingResponse> {
        let playbooks = self.playbooks.read().await;
        let playbook = playbooks
            .get(&request.playbook_id)
            .ok_or_else(|| anyhow!("playbook not found: {}", request.playbook_id))?;

        let mut binding = if let Some(binding_yaml) = request.binding_yaml.as_ref() {
            load_binding_from_yaml(binding_yaml)
                .map_err(|error| anyhow!("invalid binding yaml: {error}"))?
        } else if let Some(binding_name) = request.binding_name.as_ref() {
            self.get_binding(binding_name).await?
        } else {
            return Err(anyhow!("test_binding requires binding_yaml or binding_name"));
        };

        compile_binding_queries(&mut binding);
        let validation = validate_binding_for_playbook(playbook, &binding);

        let query_request = request.sample_query.clone().unwrap_or_else(|| QueryRequest {
            playbook_id: request.playbook_id.clone(),
            subject_id: None,
            binding_name: request.binding_name.clone(),
            resolve: plan_ir::ResolveEntityRequest {
                entity: playbook
                    .entities
                    .first()
                    .map(|entity| entity.name.clone())
                    .unwrap_or_else(|| "crm_user".into()),
                by_name: Some("Alex Anderson".into()),
                by_identifier: None,
            },
            count: playbook
                .entity_relationships
                .first()
                .map(|relationship| plan_ir::CountRelationshipRequest {
                    relationship: relationship.relationship_name.clone(),
                    object_entity: Some(relationship.object_entity_name.clone()),
                }),
            list: None,
        });

        let plan = compile_query_request(playbook, &query_request).context("compile plan failed")?;
        let mut execution_error = None;
        let mut proof = None;

        if request.execute.unwrap_or(false) && validation.valid {
            match self.execute_plan_with_binding(&plan, &binding).await {
                Ok(envelope) => proof = Some(envelope),
                Err(error) => execution_error = Some(error.to_string()),
            }
        }

        Ok(TestBindingResponse {
            validation,
            plan,
            proof,
            execution_error,
        })
    }

    // Save a binding YAML file for a playbook and reload the catalog.
    pub async fn save_binding(
        &self,
        playbook_id: &str,
        adapter_suffix: &str,
        binding_yaml: &str,
    ) -> Result<SaveBindingResponse> {
        let playbooks = self.playbooks.read().await;
        let playbook = playbooks
            .get(playbook_id)
            .ok_or_else(|| anyhow!("playbook not found: {playbook_id}"))?;

        let mut binding = load_binding_from_yaml(binding_yaml)
            .map_err(|error| anyhow!("invalid binding yaml: {error}"))?;
        binding.playbook_id = Some(playbook_id.to_string());
        compile_binding_queries(&mut binding);

        let validation = validate_binding_for_playbook(playbook, &binding);
        if !validation.valid {
            return Err(anyhow!(
                "binding validation failed: {}",
                validation.errors.join("; ")
            ));
        }

        let binding_stem = playbook_binding_stem(playbook_id, adapter_suffix);
        let binding_path = playbook_binding_path(&self.config.bindings_dir, playbook_id, adapter_suffix);
        save_binding_to_path(&binding_path, &binding)
            .map_err(|error| anyhow!("save binding failed: {error}"))?;

        drop(playbooks);
        self.reload_catalog().await?;

        Ok(SaveBindingResponse {
            playbook_id: playbook_id.to_string(),
            binding_name: binding_stem,
            binding_path: binding_path.to_string_lossy().to_string(),
            validation,
        })
    }

    // Suggest playbook entity to source table mappings from introspected schema.
    pub async fn suggest_bindings(
        &self,
        playbook_id: &str,
        source_id: &str,
        schema_name: Option<&str>,
    ) -> Result<SuggestBindingsResponse> {
        let playbooks = self.playbooks.read().await;
        let playbook = playbooks
            .get(playbook_id)
            .ok_or_else(|| anyhow!("playbook not found: {playbook_id}"))?;

        let introspection = self.introspect_source(source_id, schema_name).await?;
        let catalog: SourceSchemaCatalog = serde_json::from_value(
            introspection
                .schema
                .clone()
                .ok_or_else(|| anyhow!("no schema returned for source '{source_id}'"))?,
        )?;

        let table_names: Vec<String> = catalog
            .tables
            .iter()
            .map(|table| table.table_name.clone())
            .collect();
        let entity_table_suggestions = suggest_entity_table_mappings(playbook, &table_names);

        Ok(SuggestBindingsResponse {
            playbook_id: playbook_id.to_string(),
            source_id: source_id.to_string(),
            entity_table_suggestions,
            tables: catalog.tables,
        })
    }

    async fn resolve_binding(&self, plan: &Plan) -> Result<PlaybookBinding> {
        let bindings = self.bindings.read().await;
        let playbooks = self.playbooks.read().await;

        if let Some(binding_name) = plan.binding_name.as_deref() {
            return bindings
                .get(binding_name)
                .cloned()
                .ok_or_else(|| anyhow!("binding not found: {binding_name}"));
        }

        let playbook = playbooks
            .get(&plan.playbook_id)
            .ok_or_else(|| anyhow!("playbook not found: {}", plan.playbook_id))?;

        let candidate_names = binding_candidates_for_playbook(playbook);
        for candidate_name in candidate_names {
            if let Some(binding) = bindings.get(&candidate_name) {
                return Ok(binding.clone());
            }
        }

        bindings
            .get("postgres")
            .cloned()
            .ok_or_else(|| anyhow!("no binding found for playbook '{}'", plan.playbook_id))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebacAllowedRowsResponse {
    pub playbook_id: String,
    pub subject_id: String,
    pub rebac_applied: bool,
    pub allowed_rows_by_entity: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSummary {
    pub source_id: String,
    pub adapter: String,
    pub has_dsn: bool,
    pub has_instance_url: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntrospectSourceResponse {
    pub source_id: String,
    pub adapter: String,
    pub schema: Option<serde_json::Value>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposeBindingResponse {
    pub playbook_id: String,
    pub binding_name: String,
    pub validation: BindingValidationReport,
    pub compiled_binding_yaml: String,
    pub binding: PlaybookBinding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestBindingRequest {
    pub playbook_id: String,
    #[serde(default)]
    pub binding_name: Option<String>,
    #[serde(default)]
    pub binding_yaml: Option<String>,
    #[serde(default)]
    pub execute: Option<bool>,
    #[serde(default)]
    pub sample_query: Option<QueryRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestBindingResponse {
    pub validation: BindingValidationReport,
    pub plan: Plan,
    pub proof: Option<ProofEnvelope>,
    pub execution_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveBindingResponse {
    pub playbook_id: String,
    pub binding_name: String,
    pub binding_path: String,
    pub validation: BindingValidationReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestBindingsResponse {
    pub playbook_id: String,
    pub source_id: String,
    pub entity_table_suggestions: std::collections::HashMap<String, String>,
    pub tables: Vec<adapter_sql::TableSchema>,
}

// Build binding lookup order for a playbook.
fn binding_candidates_for_playbook(playbook: &PlaybookDefinition) -> Vec<String> {
    let mut candidates = Vec::new();

    if let Some(default_binding) = playbook.default_binding.as_ref() {
        candidates.push(default_binding.clone());
    }

    if let Some(bindings) = playbook.bindings.as_ref() {
        for binding_name in bindings.values() {
            if !candidates.iter().any(|existing| existing == binding_name) {
                candidates.push(binding_name.clone());
            }
        }
    }

    candidates.push(playbook_binding_stem(&playbook.id, "postgres"));
    candidates.push(playbook_binding_stem(&playbook.id, "csv"));
    candidates.push(playbook_binding_stem(&playbook.id, "sql"));
    candidates.push(playbook_binding_stem(&playbook.id, "salesforce"));
    candidates.push(playbook_binding_stem(&playbook.id, "soql"));

    if let Some(entity_sources) = playbook.entity_sources.as_ref() {
        for source_suffix in entity_sources.values() {
            candidates.push(playbook_binding_stem(&playbook.id, source_suffix));
        }
    }

    candidates.push("postgres".into());
    candidates.push("salesforce".into());
    candidates
}

// Derive default saved binding stem from adapter type.
fn default_binding_name_for_playbook(
    playbook: &PlaybookDefinition,
    binding: &PlaybookBinding,
) -> String {
    if let Some(default_binding) = playbook.default_binding.as_ref() {
        return default_binding.clone();
    }

    let adapter_suffix = match binding.adapter.as_str() {
        "sql" => "postgres",
        "soql" => "salesforce",
        "mysql" => "mysql",
        "mssql" => "mssql",
        "mongodb" => "mongodb",
        "rest" => "rest",
        other => other,
    };
    playbook_binding_stem(&playbook.id, adapter_suffix)
}

// Choose which playbook entity drives binding auto-routing.
fn infer_routing_entity(playbook: &PlaybookDefinition, request: &QueryRequest) -> String {
    if let Some(object_entity) = request
        .count
        .as_ref()
        .and_then(|count| count.object_entity.as_ref())
    {
        return object_entity.clone();
    }

    if let Some(object_entity) = request
        .list
        .as_ref()
        .and_then(|list| list.object_entity.as_ref())
    {
        return object_entity.clone();
    }

    if let Some(relationship_name) = request
        .count
        .as_ref()
        .map(|count| count.relationship.as_str())
        .or_else(|| request.list.as_ref().map(|list| list.relationship.as_str()))
    {
        if let Some(relationship) = playbook
            .entity_relationships
            .iter()
            .find(|relationship| relationship.relationship_name == relationship_name)
        {
            return relationship.object_entity_name.clone();
        }
    }

    request.resolve.entity.clone()
}

// Resolve default paths relative to ag-cli workspace root.
// Replace env:VAR references in profile connection fields.
fn resolve_profile_env_refs(profile: &mut SourceProfile) {
    for source in profile.sources.values_mut() {
        if let Some(dsn) = source.dsn.as_ref() {
            source.dsn = Some(resolve_env_reference(dsn));
        }
        if let Some(instance_url) = source.instance_url.as_ref() {
            source.instance_url = Some(resolve_env_reference(instance_url));
        }
        if let Some(auth) = source.auth.as_ref() {
            source.auth = Some(resolve_env_reference(auth));
        }
        if let Some(file_path) = source.file_path.as_ref() {
            source.file_path = Some(resolve_env_reference(file_path));
        }
        if let Some(base_url) = source.base_url.as_ref() {
            source.base_url = Some(resolve_env_reference(base_url));
        }
        if let Some(database) = source.database.as_ref() {
            source.database = Some(resolve_env_reference(database));
        }
    }
}

fn resolve_env_reference(raw_value: &str) -> String {
    if let Some(env_key) = raw_value.strip_prefix("env:") {
        return std::env::var(env_key).unwrap_or_default();
    }
    raw_value.to_string()
}

// Resolve ag-cli workspace root regardless of current working directory.
pub fn resolve_workspace_root() -> PathBuf {
    if let Ok(workspace_root) = std::env::var("AG_WORKSPACE_ROOT") {
        return PathBuf::from(workspace_root);
    }

    let mut candidate = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    loop {
        if is_ag_cli_workspace_root(&candidate) {
            return candidate;
        }
        if let Some(parent) = candidate.parent() {
            candidate = parent.to_path_buf();
        } else {
            break;
        }
    }

    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

// Check whether a directory looks like the ag-cli workspace root.
fn is_ag_cli_workspace_root(candidate: &Path) -> bool {
    candidate.join("playbooks").is_dir()
        && candidate.join("bindings").is_dir()
        && candidate.join("Cargo.toml").is_file()
}

pub fn default_paths_from_workspace(workspace_root: &Path) -> RuntimeConfig {
    RuntimeConfig {
        playbooks_dir: workspace_root.join("playbooks"),
        bindings_dir: workspace_root.join("bindings"),
        profile_path: Some(workspace_root.join("profiles").join("local.yaml")),
    }
}
