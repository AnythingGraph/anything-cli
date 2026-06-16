mod engine;
mod introspect;
mod sample;
mod authoring;

pub use authoring::{mssql_authoring_guide, mysql_authoring_guide, postgres_authoring_guide};

pub use engine::{GenericSqlAdapter, SqlDialect};
pub use introspect::{
    introspect_mssql_schema, introspect_mysql_schema, introspect_postgres_schema, SourceSchemaCatalog,
    ColumnSchema, ForeignKeySchema, TableSchema,
};
pub use sample::{cap_sample_limit, sample_sql_table, DEFAULT_SOURCE_SAMPLE_LIMIT, MAX_SOURCE_SAMPLE_LIMIT};

pub struct SqlAdapter;

impl SqlAdapter {
    // Postgres adapter using the shared SQL engine.
    pub fn inner() -> GenericSqlAdapter {
        GenericSqlAdapter {
            dialect: SqlDialect::Postgres,
            adapter_type: "sql",
        }
    }
}

pub struct MysqlAdapter;

impl MysqlAdapter {
    // MySQL adapter using the shared SQL engine.
    pub fn inner() -> GenericSqlAdapter {
        GenericSqlAdapter {
            dialect: SqlDialect::Mysql,
            adapter_type: "mysql",
        }
    }
}

pub struct MssqlAdapter;

impl MssqlAdapter {
    // SQL Server adapter using the shared SQL engine.
    pub fn inner() -> GenericSqlAdapter {
        GenericSqlAdapter {
            dialect: SqlDialect::Mssql,
            adapter_type: "mssql",
        }
    }
}

use adapter_core::DataAdapter;
use async_trait::async_trait;
use binding_spec::PlaybookBinding;
use plan_ir::{PlanStep, StepResult};
use adapter_core::{ExecContext, ExecutionState, AdapterError};
use serde_json::Value;

#[async_trait]
impl DataAdapter for SqlAdapter {
    fn adapter_type(&self) -> &'static str {
        "sql"
    }

    async fn execute_step(
        &self,
        step_index: usize,
        step: &PlanStep,
        binding: &PlaybookBinding,
        context: &ExecContext,
        state: &ExecutionState,
    ) -> Result<StepResult, AdapterError> {
        Self::inner()
            .execute_step(step_index, step, binding, context, state)
            .await
    }

    async fn load_entity_rows(
        &self,
        entity_name: &str,
        binding: &PlaybookBinding,
        context: &ExecContext,
    ) -> Result<Vec<Value>, AdapterError> {
        Self::inner()
            .load_entity_rows(entity_name, binding, context)
            .await
    }
}

#[async_trait]
impl DataAdapter for MysqlAdapter {
    fn adapter_type(&self) -> &'static str {
        "mysql"
    }

    async fn execute_step(
        &self,
        step_index: usize,
        step: &PlanStep,
        binding: &PlaybookBinding,
        context: &ExecContext,
        state: &ExecutionState,
    ) -> Result<StepResult, AdapterError> {
        Self::inner()
            .execute_step(step_index, step, binding, context, state)
            .await
    }

    async fn load_entity_rows(
        &self,
        entity_name: &str,
        binding: &PlaybookBinding,
        context: &ExecContext,
    ) -> Result<Vec<Value>, AdapterError> {
        Self::inner()
            .load_entity_rows(entity_name, binding, context)
            .await
    }
}

#[async_trait]
impl DataAdapter for MssqlAdapter {
    fn adapter_type(&self) -> &'static str {
        "mssql"
    }

    async fn execute_step(
        &self,
        step_index: usize,
        step: &PlanStep,
        binding: &PlaybookBinding,
        context: &ExecContext,
        state: &ExecutionState,
    ) -> Result<StepResult, AdapterError> {
        Self::inner()
            .execute_step(step_index, step, binding, context, state)
            .await
    }

    async fn load_entity_rows(
        &self,
        entity_name: &str,
        binding: &PlaybookBinding,
        context: &ExecContext,
    ) -> Result<Vec<Value>, AdapterError> {
        Self::inner()
            .load_entity_rows(entity_name, binding, context)
            .await
    }
}
