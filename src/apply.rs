use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, anyhow};
use sqlx::{
    QueryBuilder, Transaction,
    mysql::{MySql, MySqlConnection},
};

use crate::{
    config::{TableMode, TableSeed},
    metadata::{ColumnInfo, fetch_columns},
    table_name::TableName,
    value::{SqlValue, convert_value},
};

/// TODO: add English documentation
#[derive(Debug, Default)]
pub struct ApplyOutcome {
    /// TODO: add English documentation
    pub statements: Vec<String>,
}

/// TODO: add English documentation
pub async fn apply_table(
    transaction: &mut Transaction<'_, MySql>,
    seed: &TableSeed,
    dry_run: bool,
    validate_only: bool,
    ignore_missing_tables: bool,
) -> Result<ApplyOutcome> {
    let table_name = TableName::parse(&seed.name)?;
    let columns = {
        let connection: &mut MySqlConnection = &mut *transaction;
        fetch_columns(connection, table_name.schema(), table_name.table()).await?
    };
    if columns.is_empty() {
        if ignore_missing_tables {
            tracing::warn!(
                "table `{}` not found in database schema — skipped",
                seed.name
            );
            return Ok(ApplyOutcome::default());
        }
        return Err(anyhow!(
            "table `{}` not found in database schema",
            seed.name
        ));
    }

    let column_map: HashMap<_, _> = columns
        .iter()
        .map(|col| (col.name.clone(), col.clone()))
        .collect();

    let is_generated = |col: &ColumnInfo| {
        let extra_lower = col.extra.to_ascii_lowercase();
        extra_lower.contains("stored generated")
            || extra_lower.contains("virtual generated")
            || extra_lower.contains("generated always")
    };

    let required_columns: Vec<_> = columns
        .iter()
        .filter(|col| {
            !col.is_nullable
                && col.column_default.is_none()
                && !col.extra.to_ascii_lowercase().contains("auto_increment")
                && !is_generated(col)
        })
        .collect();

    let mut outcome = ApplyOutcome {
        statements: Vec::new(),
    };

    for (row_index, row) in seed.rows.iter().enumerate() {
        // TODO: add English comment
        for key in row.keys() {
            if !column_map.contains_key(key) {
                return Err(anyhow!(
                    "unknown column `{}` in table `{}` (row {})",
                    key,
                    seed.name,
                    row_index + 1
                ));
            }
        }

        // TODO: add English comment
        for required in &required_columns {
            if !row.contains_key(&required.name) {
                return Err(anyhow!(
                    "missing required column `{}` in table `{}` (row {})",
                    required.name,
                    seed.name,
                    row_index + 1
                ));
            }
        }

        // TODO: add English comment
        let mut ordered_columns: Vec<&ColumnInfo> = Vec::new();
        let mut ordered_values: Vec<SqlValue> = Vec::new();
        for column in &columns {
            let generated = is_generated(column);
            if let Some(value) = row.get(&column.name) {
                if generated {
                    return Err(anyhow!(
                        "column `{}` in table `{}` is generated and cannot be specified (row {})",
                        column.name,
                        seed.name,
                        row_index + 1
                    ));
                }
                let converted = convert_value(column, value).with_context(|| {
                    format!(
                        "failed to convert value for column `{}` in table `{}` (row {})",
                        column.name,
                        seed.name,
                        row_index + 1
                    )
                })?;
                ordered_columns.push(column);
                ordered_values.push(converted);
            }

            if generated {
                continue;
            }
        }

        if ordered_columns.is_empty() {
            continue;
        }

        let mut builder = QueryBuilder::<MySql>::new("INSERT INTO ");
        builder.push(table_name.quoted());
        builder.push(" (");
        {
            let mut separated = builder.separated(", ");
            for column in &ordered_columns {
                separated.push(format!("`{}`", column.name));
            }
        }
        builder.push(") VALUES (");
        {
            let mut separated = builder.separated(", ");
            for value in &ordered_values {
                match value {
                    SqlValue::Null => {
                        separated.push("NULL");
                    }
                    SqlValue::String(v) => {
                        separated.push_bind(v.clone());
                    }
                    SqlValue::Int(v) => {
                        separated.push_bind(*v);
                    }
                    SqlValue::Decimal(v) => {
                        separated.push_bind(v.clone());
                    }
                    SqlValue::Float(v) => {
                        separated.push_bind(*v);
                    }
                    SqlValue::Date(date) => {
                        separated.push_bind(*date);
                    }
                    SqlValue::DateTime(datetime) => {
                        separated.push_bind(*datetime);
                    }
                    SqlValue::Time(time) => {
                        separated.push_bind(*time);
                    }
                    SqlValue::Json(json) => {
                        separated.push_bind(json.clone());
                    }
                }
            }
        }
        builder.push(")");

        if seed.mode == TableMode::UpsertUpdate {
            let update_columns: Vec<String> = if let Some(custom) = &seed.update_columns {
                let valid: HashSet<&str> =
                    ordered_columns.iter().map(|c| c.name.as_str()).collect();
                let mut resolved = Vec::new();
                for column_name in custom {
                    if !column_map.contains_key(column_name) {
                        return Err(anyhow!(
                            "update column `{}` is not part of table `{}`",
                            column_name,
                            seed.name
                        ));
                    }
                    if !valid.contains(column_name.as_str()) {
                        return Err(anyhow!(
                            "update column `{}` has no value in row {}",
                            column_name,
                            row_index + 1
                        ));
                    }
                    resolved.push(column_name.clone());
                }
                resolved
            } else {
                ordered_columns.iter().map(|c| c.name.clone()).collect()
            };

            if !update_columns.is_empty() {
                builder.push(" ON DUPLICATE KEY UPDATE ");
                let mut separated = builder.separated(", ");
                for column in update_columns {
                    separated.push(format!("`{column}` = VALUES(`{column}`)"));
                }
            }
        }

        let sql_preview = builder.sql().to_string();
        let query = builder.build();
        outcome.statements.push(sql_preview.clone());

        if dry_run || validate_only {
            continue;
        }

        {
            let connection: &mut MySqlConnection = &mut *transaction;
            query
                .execute(connection)
                .await
                .with_context(|| format!("failed to execute insert for table `{}`", seed.name))?;
        }
    }

    Ok(outcome)
}
