use anyhow::Result;
use sqlx::{
    Row,
    mysql::{MySqlConnection, MySqlRow},
};

/// TODO: add English documentation
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    /// TODO: add English documentation
    pub name: String,
    /// `information_schema.columns.data_type`
    pub data_type: String,
    /// TODO: add English documentation
    pub column_type: String,
    /// TODO: add English documentation
    pub is_nullable: bool,
    /// TODO: add English documentation
    pub column_default: Option<String>,
    /// TODO: add English documentation
    pub extra: String,
}

/// TODO: add English documentation
pub async fn fetch_columns(
    executor: &mut MySqlConnection,
    schema: &str,
    table: &str,
) -> Result<Vec<ColumnInfo>> {
    let rows: Vec<MySqlRow> = sqlx::query(
        r#"
        SELECT
            column_name AS column_name,
            CAST(data_type AS CHAR) AS data_type,
            CAST(column_type AS CHAR) AS column_type,
            CAST(is_nullable AS CHAR) AS nullable_flag,
            CAST(column_default AS CHAR) AS column_default,
            CAST(extra AS CHAR) AS extra_info
        FROM information_schema.columns
        WHERE table_schema = ? AND table_name = ?
        ORDER BY ordinal_position
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(executor)
    .await?;

    rows.into_iter()
        .map(|row| -> Result<ColumnInfo> {
            let is_nullable: String = row.try_get("nullable_flag")?;
            Ok(ColumnInfo {
                name: row.try_get("column_name")?,
                data_type: row.try_get("data_type")?,
                column_type: row.try_get("column_type")?,
                is_nullable: is_nullable.eq_ignore_ascii_case("YES"),
                column_default: row.try_get("column_default").ok(),
                extra: row.try_get("extra_info")?,
            })
        })
        .collect()
}
