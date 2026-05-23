use anyhow::{Context, Result, anyhow};
use serde_json::Value as JsonValue;
use sqlx::MySqlPool;

use crate::{
    config::{TableMode, TableRow, TableSeed},
    metadata::{ColumnInfo, fetch_columns},
    table_name::TableName,
};

/// TODO: add English documentation
pub async fn export_tables(pool: &MySqlPool, tables: &[String]) -> Result<Vec<TableSeed>> {
    let mut results = Vec::new();

    for table in tables {
        let table_name = TableName::parse(table)?;
        let mut connection = pool.acquire().await.with_context(|| {
            format!("failed to acquire connection for exporting table `{table}`")
        })?;
        let columns =
            fetch_columns(connection.as_mut(), table_name.schema(), table_name.table()).await?;
        if columns.is_empty() {
            return Err(anyhow!("table `{}` does not exist", table));
        }
        let query = build_json_select(&table_name, &columns);
        let json_value: Option<JsonValue> = sqlx::query_scalar(&query)
            .fetch_one(connection.as_mut())
            .await
            .with_context(|| format!("failed to export rows for table `{table}`"))?;
        let json_value = json_value.unwrap_or_else(|| JsonValue::Array(Vec::new()));
        let rows: Vec<TableRow> = serde_yaml::from_str(&json_value.to_string())
            .with_context(|| format!("failed to decode query result for table `{table}`"))?;

        results.push(TableSeed {
            name: table.clone(),
            mode: TableMode::UpsertUpdate,
            update_columns: None,
            rows,
        });
    }

    Ok(results)
}

fn build_json_select(table_name: &TableName, columns: &[ColumnInfo]) -> String {
    if columns.is_empty() {
        return format!("SELECT JSON_ARRAY() FROM {}", table_name.quoted());
    }

    let object_pairs = columns
        .iter()
        .map(|col| format!("'{}', t.`{}`", col.name, col.name))
        .collect::<Vec<_>>()
        .join(", ");

    let order_clause = columns
        .iter()
        .map(|col| format!("t.`{}`", col.name))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "SELECT COALESCE(JSON_ARRAYAGG(row_data), JSON_ARRAY()) FROM (SELECT JSON_OBJECT({}) AS row_data FROM {} AS t ORDER BY {}) AS ordered",
        object_pairs,
        table_name.quoted(),
        order_clause
    )
}
