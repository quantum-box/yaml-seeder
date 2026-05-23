use assert_cmd::prelude::*;
use predicates::str::contains;
use serial_test::serial;
use sqlx::query;
use std::process::Command;
use std::{env, fs, io::Write, path::Path};
use tempfile::{NamedTempFile, tempdir};

type TestResult<T> = Result<T, Box<dyn std::error::Error>>;

fn database_url() -> String {
    env::var("DATABASE_URL").unwrap_or_else(|_| "mysql://root@127.0.0.1:15000".to_string())
}

async fn ensure_schema(pool: &sqlx::MySqlPool, schema: &str) -> sqlx::Result<()> {
    let create = format!("CREATE DATABASE IF NOT EXISTS `{}`", schema);
    query(&create).execute(pool).await?;
    Ok(())
}

async fn cleanup_table(pool: &sqlx::MySqlPool, full_table: &str) -> sqlx::Result<()> {
    let drop = format!("DROP TABLE IF EXISTS {full_table}");
    query(&drop).execute(pool).await?;
    Ok(())
}

async fn create_test_table(pool: &sqlx::MySqlPool, full_table: &str) -> sqlx::Result<()> {
    let ddl = format!(
        "CREATE TABLE {full_table} (
            id VARCHAR(32) NOT NULL PRIMARY KEY,
            name VARCHAR(255) NOT NULL,
            amount INT NOT NULL,
            updated_at DATETIME(6) NOT NULL
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4"
    );
    query(&ddl).execute(pool).await?;
    Ok(())
}

fn write_seed_file(path: &Path, table: &str, entries: &[(&str, &str, i32)]) -> TestResult<()> {
    let mut file = fs::File::create(path)?;
    writeln!(file, "version: 1")?;
    writeln!(file, "tables:")?;
    writeln!(file, "- name: {}", table)?;
    writeln!(file, "  rows:")?;
    for (id, name, amount) in entries {
        writeln!(file, "    - id: {}", id)?;
        writeln!(file, "      name: {}", name)?;
        writeln!(file, "      amount: {}", amount)?;
        writeln!(file, "      updated_at: 2025-01-01 00:00:00.000000")?;
    }
    Ok(())
}

#[tokio::test]
#[serial]
async fn apply_single_file_commits_rows() -> TestResult<()> {
    let db_url = database_url();
    let pool = sqlx::MySqlPool::connect(&db_url).await?;
    ensure_schema(&pool, "yaml_seeder_test").await?;

    let table = "yaml_seeder_test.apply_single";
    cleanup_table(&pool, table).await?;
    create_test_table(&pool, table).await?;

    let file = NamedTempFile::new()?;
    write_seed_file(file.path(), table, &[("item_1", "First", 100)])?;

    Command::cargo_bin("yaml-seeder")?
        .env("DATABASE_URL", &db_url)
        .arg("apply")
        .arg(file.path())
        .assert()
        .success();

    let rows: (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(&pool)
        .await?;
    assert_eq!(rows.0, 1);

    Ok(())
}

#[tokio::test]
#[serial]
async fn apply_directory_rolls_back_failed_file() -> TestResult<()> {
    let db_url = database_url();
    let pool = sqlx::MySqlPool::connect(&db_url).await?;
    ensure_schema(&pool, "yaml_seeder_test").await?;

    let table = "yaml_seeder_test.apply_directory";
    cleanup_table(&pool, table).await?;
    create_test_table(&pool, table).await?;

    let dir = tempdir()?;
    let valid = dir.path().join("001-valid.yaml");
    write_seed_file(valid.as_path(), table, &[("item_1", "Valid", 1)])?;

    let invalid = dir.path().join("002-invalid.yaml");
    let mut file = fs::File::create(&invalid)?;
    writeln!(file, "version: 1")?;
    writeln!(file, "tables:")?;
    writeln!(file, "- name: {}", table)?;
    writeln!(file, "  rows:")?;
    writeln!(file, "    - id: bad")?; // missing required fields to trigger validation error

    Command::cargo_bin("yaml-seeder")?
        .env("DATABASE_URL", &db_url)
        .arg("apply")
        .arg(dir.path())
        .assert()
        .failure()
        .stderr(contains("missing required column"));

    let rows: (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        rows.0, 1,
        "entries from the first file should remain committed"
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn validate_only_does_not_commit_changes() -> TestResult<()> {
    let db_url = database_url();
    let pool = sqlx::MySqlPool::connect(&db_url).await?;
    ensure_schema(&pool, "yaml_seeder_test").await?;

    let table = "yaml_seeder_test.validate_only";
    cleanup_table(&pool, table).await?;
    create_test_table(&pool, table).await?;

    let file = NamedTempFile::new()?;
    write_seed_file(
        file.path(),
        table,
        &[("item_1", "First", 10), ("item_2", "Second", 20)],
    )?;

    Command::cargo_bin("yaml-seeder")?
        .env("DATABASE_URL", &db_url)
        .arg("apply")
        .arg(file.path())
        .arg("--validate-only")
        .assert()
        .success();

    let rows: (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(&pool)
        .await?;
    assert_eq!(rows.0, 0, "validate-only should not commit data");

    Ok(())
}
