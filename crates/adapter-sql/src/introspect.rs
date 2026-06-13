use crate::engine::SqlDialect;
use adapter_core::AdapterError;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSchemaCatalog {
    pub adapter: String,
    pub schema_name: String,
    pub tables: Vec<TableSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSchema {
    pub table_name: String,
    pub columns: Vec<ColumnSchema>,
    pub foreign_keys: Vec<ForeignKeySchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnSchema {
    pub column_name: String,
    pub data_type: String,
    pub is_nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKeySchema {
    pub column_name: String,
    pub foreign_table_name: String,
    pub foreign_column_name: String,
}

// Introspect Postgres tables, columns, and foreign keys.
pub async fn introspect_postgres_schema(
    dsn: &str,
    schema_name: Option<&str>,
) -> Result<SourceSchemaCatalog, AdapterError> {
    let schema_name = schema_name.unwrap_or("public");
    introspect_information_schema(SqlDialect::Postgres, dsn, schema_name).await
}

// Introspect MySQL tables, columns, and foreign keys.
pub async fn introspect_mysql_schema(
    dsn: &str,
    schema_name: Option<&str>,
) -> Result<SourceSchemaCatalog, AdapterError> {
    let schema_name_owned = schema_name
        .map(|value| value.to_string())
        .or_else(|| std::env::var("AG_MYSQL_DATABASE").ok())
        .unwrap_or_else(|| "mysql".to_string());
    introspect_information_schema(SqlDialect::Mysql, dsn, &schema_name_owned).await
}

// Introspect SQL Server tables, columns, and foreign keys.
pub async fn introspect_mssql_schema(
    dsn: &str,
    schema_name: Option<&str>,
) -> Result<SourceSchemaCatalog, AdapterError> {
    let schema_name = schema_name.unwrap_or("dbo");
    introspect_mssql_with_tiberius(dsn, schema_name).await
}

async fn introspect_information_schema(
    dialect: SqlDialect,
    dsn: &str,
    schema_name: &str,
) -> Result<SourceSchemaCatalog, AdapterError> {
    let adapter_name = match dialect {
        SqlDialect::Postgres => "sql",
        SqlDialect::Mysql => "mysql",
        SqlDialect::Mssql => "mssql",
    };

    let column_query = "
        SELECT column_info.table_name, column_info.column_name, column_info.data_type, column_info.is_nullable
        FROM information_schema.columns AS column_info
        JOIN information_schema.tables AS table_info
          ON column_info.table_schema = table_info.table_schema
         AND column_info.table_name = table_info.table_name
        WHERE column_info.table_schema = ?
          AND table_info.table_type = 'BASE TABLE'
        ORDER BY column_info.table_name, column_info.ordinal_position
        ";

    let foreign_key_query = "
        SELECT
            source_table.table_name AS table_name,
            source_column.column_name AS column_name,
            target_table.table_name AS foreign_table_name,
            target_column.column_name AS foreign_column_name
        FROM information_schema.table_constraints AS table_constraint
        JOIN information_schema.key_column_usage AS source_column
          ON table_constraint.constraint_name = source_column.constraint_name
         AND table_constraint.table_schema = source_column.table_schema
        JOIN information_schema.constraint_column_usage AS target_column
          ON table_constraint.constraint_name = target_column.constraint_name
         AND table_constraint.table_schema = target_column.table_schema
        JOIN information_schema.tables AS source_table
          ON source_table.table_name = source_column.table_name
         AND source_table.table_schema = source_column.table_schema
        JOIN information_schema.tables AS target_table
          ON target_table.table_name = target_column.table_name
         AND target_table.table_schema = target_column.table_schema
        WHERE table_constraint.constraint_type = 'FOREIGN KEY'
          AND table_constraint.table_schema = ?
        ORDER BY source_table.table_name, source_column.column_name
        ";

    match dialect {
        SqlDialect::Postgres => {
            use sqlx::postgres::PgPoolOptions;
            let pool = PgPoolOptions::new()
                .max_connections(2)
                .connect(dsn)
                .await
                .map_err(|error| AdapterError::Message(format!("postgres connect failed: {error}")))?;
            let column_rows = sqlx::query(column_query)
                .bind(schema_name)
                .fetch_all(&pool)
                .await
                .map_err(|error| AdapterError::Message(format!("schema columns query failed: {error}")))?;
            let foreign_key_rows = sqlx::query(foreign_key_query)
                .bind(schema_name)
                .fetch_all(&pool)
                .await
                .map_err(|error| AdapterError::Message(format!("schema foreign keys query failed: {error}")))?;
            let column_rows = convert_postgres_rows_to_schema_rows(&column_rows)?;
            let foreign_key_rows = convert_postgres_rows_to_foreign_key_rows(&foreign_key_rows)?;
            build_schema_catalog_from_rows(adapter_name, schema_name, column_rows, foreign_key_rows)
        }
        SqlDialect::Mysql => {
            use sqlx::mysql::MySqlPoolOptions;
            let pool = MySqlPoolOptions::new()
                .max_connections(2)
                .connect(dsn)
                .await
                .map_err(|error| AdapterError::Message(format!("mysql connect failed: {error}")))?;
            let column_rows = sqlx::query(column_query)
                .bind(schema_name)
                .fetch_all(&pool)
                .await
                .map_err(|error| AdapterError::Message(format!("schema columns query failed: {error}")))?;
            let foreign_key_rows = sqlx::query(foreign_key_query)
                .bind(schema_name)
                .fetch_all(&pool)
                .await
                .map_err(|error| AdapterError::Message(format!("schema foreign keys query failed: {error}")))?;
            let column_rows = convert_mysql_rows_to_schema_rows(&column_rows)?;
            let foreign_key_rows = convert_mysql_rows_to_foreign_key_rows(&foreign_key_rows)?;
            build_schema_catalog_from_rows(adapter_name, schema_name, column_rows, foreign_key_rows)
        }
        SqlDialect::Mssql => unreachable!("mssql uses tiberius introspection"),
    }
}

#[derive(Debug, Clone)]
struct SchemaRow {
    table_name: String,
    column_name: String,
    data_type: String,
    is_nullable: String,
}

#[derive(Debug, Clone)]
struct ForeignKeyRow {
    table_name: String,
    column_name: String,
    foreign_table_name: String,
    foreign_column_name: String,
}

// Introspect SQL Server schema using tiberius.
async fn introspect_mssql_with_tiberius(
    dsn: &str,
    schema_name: &str,
) -> Result<SourceSchemaCatalog, AdapterError> {
    let column_query = format!(
        "
        SELECT column_info.table_name, column_info.column_name, column_info.data_type, column_info.is_nullable
        FROM information_schema.columns AS column_info
        JOIN information_schema.tables AS table_info
          ON column_info.table_schema = table_info.table_schema
         AND column_info.table_name = table_info.table_name
        WHERE column_info.table_schema = '{schema_name}'
          AND table_info.table_type = 'BASE TABLE'
        ORDER BY column_info.table_name, column_info.ordinal_position
        "
    );
    let foreign_key_query = format!(
        "
        SELECT
            source_table.table_name AS table_name,
            source_column.column_name AS column_name,
            target_table.table_name AS foreign_table_name,
            target_column.column_name AS foreign_column_name
        FROM information_schema.table_constraints AS table_constraint
        JOIN information_schema.key_column_usage AS source_column
          ON table_constraint.constraint_name = source_column.constraint_name
         AND table_constraint.table_schema = source_column.table_schema
        JOIN information_schema.constraint_column_usage AS target_column
          ON table_constraint.constraint_name = target_column.constraint_name
         AND table_constraint.table_schema = target_column.table_schema
        JOIN information_schema.tables AS source_table
          ON source_table.table_name = source_column.table_name
         AND source_table.table_schema = source_column.table_schema
        JOIN information_schema.tables AS target_table
          ON target_table.table_name = target_column.table_name
         AND target_table.table_schema = target_column.table_schema
        WHERE table_constraint.constraint_type = 'FOREIGN KEY'
          AND table_constraint.table_schema = '{schema_name}'
        ORDER BY source_table.table_name, source_column.column_name
        "
    );

    let column_maps = run_mssql_schema_query(dsn, &column_query).await?;
    let foreign_key_maps = run_mssql_schema_query(dsn, &foreign_key_query).await?;

    let column_rows: Vec<SchemaRow> = column_maps
        .into_iter()
        .map(|row| SchemaRow {
            table_name: row.get("table_name").cloned().unwrap_or_default(),
            column_name: row.get("column_name").cloned().unwrap_or_default(),
            data_type: row.get("data_type").cloned().unwrap_or_default(),
            is_nullable: row.get("is_nullable").cloned().unwrap_or_default(),
        })
        .collect();

    let foreign_key_rows: Vec<ForeignKeyRow> = foreign_key_maps
        .into_iter()
        .map(|row| ForeignKeyRow {
            table_name: row.get("table_name").cloned().unwrap_or_default(),
            column_name: row.get("column_name").cloned().unwrap_or_default(),
            foreign_table_name: row.get("foreign_table_name").cloned().unwrap_or_default(),
            foreign_column_name: row.get("foreign_column_name").cloned().unwrap_or_default(),
        })
        .collect();

    build_schema_catalog_from_rows("mssql", schema_name, column_rows, foreign_key_rows)
}

async fn run_mssql_schema_query(
    dsn: &str,
    query_text: &str,
) -> Result<Vec<HashMap<String, String>>, AdapterError> {
    use futures_util::TryStreamExt;
    use tiberius::{Client, Config, QueryItem};
    use tokio::net::TcpStream;
    use tokio_util::compat::TokioAsyncWriteCompatExt;

    let config = Config::from_jdbc_string(dsn)
        .map_err(|error| AdapterError::Message(format!("mssql parse dsn failed: {error}")))?;

    let tcp = TcpStream::connect(config.get_addr())
        .await
        .map_err(|error| AdapterError::Message(format!("mssql connect failed: {error}")))?;
    let mut client = Client::connect(config, tcp.compat_write())
        .await
        .map_err(|error| AdapterError::Message(format!("mssql client connect failed: {error}")))?;

    let mut stream = client
        .query(query_text, &[])
        .await
        .map_err(|error| AdapterError::Message(format!("mssql schema query failed: {error}")))?;

    let mut rows = Vec::new();
    while let Some(item) = stream
        .try_next()
        .await
        .map_err(|error| AdapterError::Message(format!("mssql schema stream failed: {error}")))?
    {
        if let QueryItem::Row(row) = item {
            let mut map = HashMap::new();
            for index in 0..row.columns().len() {
                let column_name = row.columns()[index].name().to_string();
                let value: Option<String> = row
                    .try_get::<&str, _>(index)
                    .ok()
                    .flatten()
                    .map(|text| text.to_string());
                map.insert(column_name, value.unwrap_or_default());
            }
            rows.push(map);
        }
    }
    Ok(rows)
}

fn convert_postgres_rows_to_schema_rows(
    rows: &[sqlx::postgres::PgRow],
) -> Result<Vec<SchemaRow>, AdapterError> {
    let mut converted = Vec::new();
    for row in rows {
        converted.push(SchemaRow {
            table_name: read_postgres_row_column(row, "table_name")?,
            column_name: read_postgres_row_column(row, "column_name")?,
            data_type: read_postgres_row_column(row, "data_type")?,
            is_nullable: read_postgres_row_column(row, "is_nullable")?,
        });
    }
    Ok(converted)
}

fn convert_mysql_rows_to_schema_rows(
    rows: &[sqlx::mysql::MySqlRow],
) -> Result<Vec<SchemaRow>, AdapterError> {
    let mut converted = Vec::new();
    for row in rows {
        converted.push(SchemaRow {
            table_name: read_mysql_row_column(row, "table_name")?,
            column_name: read_mysql_row_column(row, "column_name")?,
            data_type: read_mysql_row_column(row, "data_type")?,
            is_nullable: read_mysql_row_column(row, "is_nullable")?,
        });
    }
    Ok(converted)
}

fn convert_postgres_rows_to_foreign_key_rows(
    rows: &[sqlx::postgres::PgRow],
) -> Result<Vec<ForeignKeyRow>, AdapterError> {
    let mut converted = Vec::new();
    for row in rows {
        converted.push(ForeignKeyRow {
            table_name: read_postgres_row_column(row, "table_name")?,
            column_name: read_postgres_row_column(row, "column_name")?,
            foreign_table_name: read_postgres_row_column(row, "foreign_table_name")?,
            foreign_column_name: read_postgres_row_column(row, "foreign_column_name")?,
        });
    }
    Ok(converted)
}

fn convert_mysql_rows_to_foreign_key_rows(
    rows: &[sqlx::mysql::MySqlRow],
) -> Result<Vec<ForeignKeyRow>, AdapterError> {
    let mut converted = Vec::new();
    for row in rows {
        converted.push(ForeignKeyRow {
            table_name: read_mysql_row_column(row, "table_name")?,
            column_name: read_mysql_row_column(row, "column_name")?,
            foreign_table_name: read_mysql_row_column(row, "foreign_table_name")?,
            foreign_column_name: read_mysql_row_column(row, "foreign_column_name")?,
        });
    }
    Ok(converted)
}

fn read_postgres_row_column(row: &sqlx::postgres::PgRow, column_name: &str) -> Result<String, AdapterError> {
    let value: Option<String> = row.try_get(column_name).map_err(|error| {
        AdapterError::Message(format!("read {column_name} failed: {error}"))
    })?;
    Ok(value.unwrap_or_default())
}

fn read_mysql_row_column(row: &sqlx::mysql::MySqlRow, column_name: &str) -> Result<String, AdapterError> {
    let value: Option<String> = row.try_get(column_name).map_err(|error| {
        AdapterError::Message(format!("read {column_name} failed: {error}"))
    })?;
    Ok(value.unwrap_or_default())
}

fn build_schema_catalog_from_rows(
    adapter_name: &str,
    schema_name: &str,
    column_rows: Vec<SchemaRow>,
    foreign_key_rows: Vec<ForeignKeyRow>,
) -> Result<SourceSchemaCatalog, AdapterError> {
    let mut tables_by_name: HashMap<String, TableSchema> = HashMap::new();

    for row in column_rows {
        let table_entry = tables_by_name.entry(row.table_name.clone()).or_insert(TableSchema {
            table_name: row.table_name.clone(),
            columns: Vec::new(),
            foreign_keys: Vec::new(),
        });
        table_entry.columns.push(ColumnSchema {
            column_name: row.column_name,
            data_type: row.data_type,
            is_nullable: row.is_nullable.eq_ignore_ascii_case("YES"),
        });
    }

    for row in foreign_key_rows {
        let table_entry = tables_by_name.entry(row.table_name.clone()).or_insert(TableSchema {
            table_name: row.table_name.clone(),
            columns: Vec::new(),
            foreign_keys: Vec::new(),
        });
        table_entry.foreign_keys.push(ForeignKeySchema {
            column_name: row.column_name,
            foreign_table_name: row.foreign_table_name,
            foreign_column_name: row.foreign_column_name,
        });
    }

    let mut tables: Vec<TableSchema> = tables_by_name.into_values().collect();
    tables.sort_by(|left, right| left.table_name.cmp(&right.table_name));

    Ok(SourceSchemaCatalog {
        adapter: adapter_name.to_string(),
        schema_name: schema_name.to_string(),
        tables,
    })
}
