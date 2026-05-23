use std::str::FromStr;

use anyhow::{Context, Result, bail};
use bigdecimal::BigDecimal;
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;

use crate::metadata::ColumnInfo;

/// TODO: add English documentation
#[derive(Debug, Clone)]
pub enum SqlValue {
    /// TODO: add English documentation
    Null,
    /// TODO: add English documentation
    String(String),
    /// TODO: add English documentation
    Int(i64),
    /// TODO: add English documentation
    Decimal(BigDecimal),
    /// TODO: add English documentation
    Float(f64),
    /// TODO: add English documentation
    Date(NaiveDate),
    /// TODO: add English documentation
    DateTime(NaiveDateTime),
    /// TODO: add English documentation
    Time(NaiveTime),
    /// TODO: add English documentation
    Json(JsonValue),
}

/// TODO: add English documentation
pub fn convert_value(column: &ColumnInfo, value: &YamlValue) -> Result<SqlValue> {
    if value.is_null() {
        return Ok(SqlValue::Null);
    }

    let data_type = column.data_type.to_ascii_lowercase();
    match data_type.as_str() {
        "varchar" | "char" | "text" | "mediumtext" | "longtext" | "tinytext" => {
            Ok(SqlValue::String(expect_string(value, column)?))
        }
        "json" => convert_json(value),
        "int" | "integer" | "mediumint" | "smallint" | "tinyint" | "bigint" => {
            if let Some(n) = value.as_i64() {
                return Ok(SqlValue::Int(n));
            }
            if let Some(b) = value.as_bool() {
                return Ok(SqlValue::Int(if b { 1 } else { 0 }));
            }
            let s = expect_string(value, column)?;
            if column.column_type.to_ascii_lowercase().contains("unsigned") {
                let parsed = s.parse::<u64>().with_context(|| {
                    format!(
                        "failed to parse unsigned integer for column {}",
                        column.name
                    )
                })?;
                return Ok(SqlValue::Int(parsed as i64));
            }
            let parsed = s
                .parse::<i64>()
                .with_context(|| format!("failed to parse integer for column {}", column.name))?;
            Ok(SqlValue::Int(parsed))
        }
        "decimal" | "numeric" | "dec" | "fixed" => {
            let decimal = match value {
                YamlValue::Number(num) => BigDecimal::from_str(&num.to_string())?,
                YamlValue::String(s) => BigDecimal::from_str(s).with_context(|| {
                    format!("failed to parse decimal for column {}", column.name)
                })?,
                _ => bail!("column {} expects decimal value", column.name),
            };
            Ok(SqlValue::Decimal(decimal))
        }
        "double" | "float" | "real" => {
            let number = match value {
                YamlValue::Number(num) => num.as_f64().ok_or_else(|| {
                    anyhow::anyhow!("failed to convert number to f64 for column {}", column.name)
                })?,
                YamlValue::String(s) => s
                    .parse::<f64>()
                    .with_context(|| format!("failed to parse float for column {}", column.name))?,
                _ => bail!("column {} expects float value", column.name),
            };
            Ok(SqlValue::Float(number))
        }
        "date" => {
            let s = expect_string(value, column)?;
            let parsed = NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                .or_else(|_| NaiveDate::parse_from_str(&s, "%Y/%m/%d"))
                .with_context(|| format!("failed to parse date for column {}", column.name))?;
            Ok(SqlValue::Date(parsed))
        }
        "datetime" | "timestamp" => {
            let s = expect_string(value, column)?;
            let parsed = parse_datetime(&s)
                .with_context(|| format!("failed to parse datetime for column {}", column.name))?;
            Ok(SqlValue::DateTime(parsed))
        }
        "time" => {
            let s = expect_string(value, column)?;
            let parsed = NaiveTime::parse_from_str(&s, "%H:%M:%S")
                .or_else(|_| NaiveTime::parse_from_str(&s, "%H:%M"))
                .with_context(|| format!("failed to parse time for column {}", column.name))?;
            Ok(SqlValue::Time(parsed))
        }
        "enum" => {
            let value_str = expect_string(value, column)?;
            let allowed = parse_enum_variants(&column.column_type)?;
            if !allowed.iter().any(|candidate| candidate == &value_str) {
                bail!(
                    "value '{}' is not part of enum {} (allowed: {:?})",
                    value_str,
                    column.name,
                    allowed
                );
            }
            Ok(SqlValue::String(value_str))
        }
        other => {
            // TODO: add English comment
            let s = expect_string(value, column)?;
            tracing::warn!(
                column = %column.name,
                data_type = other,
                "falling back to string binding for unsupported type"
            );
            Ok(SqlValue::String(s))
        }
    }
}

fn expect_string(value: &YamlValue, column: &ColumnInfo) -> Result<String> {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| value.as_i64().map(|v| v.to_string()))
        .or_else(|| value.as_u64().map(|v| v.to_string()))
        .or_else(|| value.as_f64().map(|v| v.to_string()))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "column {} expects string-compatible value but got {:?}",
                column.name,
                value
            )
        })
}

fn convert_json(value: &YamlValue) -> Result<SqlValue> {
    let json = match value {
        YamlValue::String(s) => serde_json::from_str::<JsonValue>(s)
            .or_else(|_| serde_json::to_value(s))
            .with_context(|| "failed to parse JSON string")?,
        other => serde_json::to_value(other)
            .with_context(|| "failed to convert YAML value into JSON value")?,
    };
    Ok(SqlValue::Json(json))
}

fn parse_datetime(input: &str) -> Result<NaiveDateTime> {
    if let Ok(dt) = NaiveDateTime::parse_from_str(input, "%Y-%m-%d %H:%M:%S") {
        return Ok(dt);
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(input, "%Y-%m-%d %H:%M:%S%.f") {
        return Ok(dt);
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(input, "%Y/%m/%d %H:%M:%S") {
        return Ok(dt);
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(input) {
        return Ok(dt.naive_utc());
    }
    bail!("unsupported datetime format: {}", input)
}

fn parse_enum_variants(column_type: &str) -> Result<Vec<String>> {
    let lower = column_type.to_ascii_lowercase();
    if !lower.starts_with("enum(") {
        bail!("column type is not enum: {}", column_type);
    }
    let inner = column_type
        .trim()
        .trim_start_matches("enum(")
        .trim_end_matches(')');
    if inner.is_empty() {
        return Ok(vec![]);
    }
    let values = inner
        .split(',')
        .map(|part| part.trim().trim_matches('\'').replace("\\'", "'"))
        .collect();
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column_info(name: &str, data_type: &str, column_type: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.to_string(),
            data_type: data_type.to_string(),
            column_type: column_type.to_string(),
            is_nullable: false,
            column_default: None,
            extra: String::new(),
        }
    }

    #[test]
    fn convert_varchar_returns_string() {
        let column = column_info("name", "varchar", "varchar(255)");
        let yaml = YamlValue::from("Alice");
        match convert_value(&column, &yaml).expect("convert") {
            SqlValue::String(s) => assert_eq!(s, "Alice"),
            other => panic!("expected string value, got {other:?}"),
        }
    }

    #[test]
    fn convert_int_parses_numbers() {
        let column = column_info("amount", "int", "int");
        let yaml = YamlValue::from(42);
        match convert_value(&column, &yaml).expect("convert") {
            SqlValue::Int(v) => assert_eq!(v, 42),
            other => panic!("expected int value, got {other:?}"),
        }
    }

    #[test]
    fn convert_decimal_handles_string_input() {
        let column = column_info("price", "decimal", "decimal(10,2)");
        let yaml = YamlValue::from("12.34");
        match convert_value(&column, &yaml).expect("convert") {
            SqlValue::Decimal(value) => {
                let expected = BigDecimal::from_str("12.34").unwrap();
                assert_eq!(value, expected);
            }
            other => panic!("expected decimal, got {other:?}"),
        }
    }

    #[test]
    fn convert_json_accepts_inline_yaml() {
        let column = column_info("metadata", "json", "json");
        let yaml = serde_yaml::from_str("{ flag: true }").unwrap();
        match convert_value(&column, &yaml).expect("convert") {
            SqlValue::Json(value) => {
                assert_eq!(value["flag"], JsonValue::Bool(true));
            }
            other => panic!("expected json, got {other:?}"),
        }
    }

    #[test]
    fn convert_enum_validates_allowed_variants() {
        let column = column_info("status", "enum", "enum('draft','published')");
        let valid = YamlValue::from("draft");
        assert!(convert_value(&column, &valid).is_ok());

        let invalid = YamlValue::from("archived");
        let err = convert_value(&column, &invalid).unwrap_err();
        assert!(err.to_string().contains("not part of enum"));
    }

    #[test]
    fn convert_datetime_parses_known_formats() {
        let column = column_info("created_at", "datetime", "datetime");
        let yaml = YamlValue::from("2025-09-19 12:34:56");
        match convert_value(&column, &yaml).expect("convert") {
            SqlValue::DateTime(value) => {
                assert_eq!(
                    value.format("%Y-%m-%d %H:%M:%S").to_string(),
                    "2025-09-19 12:34:56"
                );
            }
            other => panic!("expected datetime, got {other:?}"),
        }
    }

    #[test]
    fn convert_value_respects_nulls() {
        let column = column_info("name", "varchar", "varchar(255)");
        let yaml = YamlValue::Null;
        assert!(matches!(
            convert_value(&column, &yaml).unwrap(),
            SqlValue::Null
        ));
    }
}
