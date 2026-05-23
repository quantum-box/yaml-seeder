mod apply;
mod cli;
mod config;
mod export;
mod metadata;
mod table_name;
mod value;

use std::{
    fs,
    fs::File,
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::Parser;
use sqlx::mysql::MySqlPoolOptions;
use tracing_subscriber::{
    EnvFilter, Layer, filter::LevelFilter, layer::SubscriberExt, util::SubscriberInitExt,
};

use crate::{
    apply::apply_table,
    cli::{ApplyArgs, Cli, Command, CreateArgs, Environment},
    config::SeedFile,
    export::export_tables,
};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("エラー: {error:?}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let Cli {
        debug,
        database_url,
        command,
    } = Cli::parse();
    init_tracing(debug);

    match command {
        Command::Create(args) => handle_create(args)?,
        Command::Apply(args) => {
            let (env, target_path) = parse_apply_inputs(&args)?;
            let database_url = resolve_database_url(database_url.clone(), env)?;
            let pool = connect_pool(&database_url).await?;
            handle_apply(
                &pool,
                args.dry_run,
                args.validate_only,
                args.ignore_missing_tables,
                &target_path,
            )
            .await?;
            pool.close().await;
        }
        Command::Export(args) => {
            let env = args.env.unwrap_or(Environment::Dev);
            let database_url = resolve_database_url(database_url.clone(), env)?;
            let pool = connect_pool(&database_url).await?;
            handle_export(&pool, &args.tables, &args.file, args.pretty).await?;
            pool.close().await;
        }
    }

    Ok(())
}

fn resolve_database_url(cli_database_url: Option<String>, env: Environment) -> Result<String> {
    if let Some(url) = cli_database_url {
        return Ok(url);
    }

    match env {
        Environment::Dev => {
            if let Ok(url) = std::env::var(env.database_env_var()) {
                return Ok(url);
            }
            Ok("mysql://root@127.0.0.1:15000".to_string())
        }
        Environment::Prod => {
            if let Ok(url) = std::env::var(env.database_env_var()) {
                return Ok(url);
            }
            bail!(
                "{} または --database-url が設定されていません。",
                env.database_env_var()
            );
        }
        Environment::TidbPlayground => {
            if let Ok(url) = std::env::var(env.database_env_var()) {
                return Ok(url);
            }
            Ok("mysql://root@127.0.0.1:4000".to_string())
        }
    }
}

fn parse_apply_inputs(args: &ApplyArgs) -> Result<(Environment, PathBuf)> {
    match args.env_and_path.as_slice() {
        [single] => {
            if Environment::parse_arg(single).is_some() {
                bail!(
                    "シードファイルまたはディレクトリのパスを指定してください。\
                     例: yaml-seeder apply dev scripts/seeds/n1-seed"
                );
            }
            Ok((Environment::Dev, PathBuf::from(single)))
        }
        [env, path] => {
            let environment = if let Some(value) = Environment::parse_arg(env) {
                value
            } else {
                bail!(
                    "環境の指定が不正です: {}. 利用可能な値: dev, prod, tidb-playground",
                    env
                );
            };
            if path.is_empty() {
                bail!(
                    "シードファイルまたはディレクトリのパスを指定してください。\
                     例: yaml-seeder apply {} scripts/seeds/n1-seed",
                    env
                );
            }
            Ok((environment, PathBuf::from(path)))
        }
        [] => bail!(
            "シードファイルまたはディレクトリのパスを指定してください。\
             例: yaml-seeder apply scripts/seeds/n1-seed"
        ),
        _ => bail!(
            "引数の形式が不正です。`yaml-seeder apply [dev|prod|tidb-playground] <PATH>` の形式で指定してください。"
        ),
    }
}

fn init_tracing(debug: bool) {
    if debug {
        init_debug_tracing();
    } else {
        init_info_tracing();
    }
}

fn init_debug_tracing() {
    tracing_subscriber::registry()
        .with(EnvFilter::new("debug"))
        .with(
            tracing_subscriber::fmt::layer()
                .with_line_number(true)
                .with_filter(LevelFilter::DEBUG),
        )
        .init();
}

fn init_info_tracing() {
    tracing_subscriber::registry()
        .with(EnvFilter::new("info"))
        .with(
            tracing_subscriber::fmt::layer()
                .with_line_number(true)
                .with_filter(LevelFilter::INFO),
        )
        .init();
}

async fn connect_pool(database_url: &str) -> Result<sqlx::MySqlPool> {
    MySqlPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
        .with_context(|| format!("データベースに接続できませんでした: {database_url}"))
}

async fn handle_apply(
    pool: &sqlx::MySqlPool,
    dry_run: bool,
    validate_only: bool,
    ignore_missing_tables: bool,
    path: &Path,
) -> Result<()> {
    let targets = collect_seed_targets(path)?;
    if targets.is_empty() {
        tracing::info!(
            "{} に対象となるYAMLファイルが見つかりませんでした。",
            path.display()
        );
        return Ok(());
    }

    tracing::info!(
        "=== YAMLシード投入 (dry-run: {}, validate-only: {}) ===",
        dry_run,
        validate_only
    );

    let mut processed_any_table = false;

    for (index, target) in targets.iter().enumerate() {
        tracing::info!(
            "({}/{}) {} を処理します",
            index + 1,
            targets.len(),
            target.display()
        );

        let seed_file = load_seed_file(target)?;
        if seed_file.tables.is_empty() {
            tracing::info!("   対象テーブルが定義されていないためスキップします。");
            continue;
        }

        processed_any_table = true;
        process_seed_file(
            pool,
            dry_run,
            validate_only,
            ignore_missing_tables,
            target,
            seed_file,
        )
        .await?;
    }

    if !processed_any_table {
        tracing::info!("処理対象となるテーブル定義が存在しませんでした。");
        return Ok(());
    }

    if dry_run {
        tracing::info!("ドライランのためSQLは実行していません。");
    } else if validate_only {
        tracing::info!("バリデーションのみ実施しました。SQLは実行していません。");
    } else {
        tracing::info!("全テーブルの投入が完了しました。");
    }

    Ok(())
}

async fn handle_export(
    pool: &sqlx::MySqlPool,
    tables: &[String],
    output_path: &std::path::Path,
    pretty: bool,
) -> Result<()> {
    if tables.is_empty() {
        tracing::info!("エクスポート対象のテーブルが指定されていません。");
        return Ok(());
    }

    let exported = export_tables(pool, tables).await?;
    let seed = SeedFile {
        version: 1,
        tables: exported,
    };

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("ディレクトリを作成できませんでした: {}", parent.display()))?;
    }

    let file = File::create(output_path).with_context(|| {
        format!(
            "出力ファイルを作成できませんでした: {}",
            output_path.display()
        )
    })?;
    let mut writer = BufWriter::new(file);

    if pretty {
        let yaml = serde_yaml::to_string(&seed)?;
        writer.write_all(yaml.as_bytes())?;
    } else {
        serde_yaml::to_writer(&mut writer, &seed)?;
    }

    writer.flush()?;
    tracing::info!(
        "テーブル{}件を {} にエクスポートしました。",
        tables.len(),
        output_path.display()
    );
    Ok(())
}

fn handle_create(args: CreateArgs) -> Result<()> {
    let CreateArgs {
        name,
        directory,
        number,
        width,
    } = args;

    fs::create_dir_all(&directory).with_context(|| {
        format!(
            "ディレクトリを作成できませんでした: {}",
            directory.display()
        )
    })?;

    let (next_sequence, resolved_width) = resolve_sequence(&directory, number, width)?;
    let slug = slugify(&name);
    let filename = format!("{next_sequence:0resolved_width$}-{slug}.yaml");
    let path = directory.join(filename);

    if path.exists() {
        bail!(
            "指定された番号と名前のシードファイルが既に存在します: {}",
            path.display()
        );
    }

    let mut file = File::create(&path)
        .with_context(|| format!("シードファイルを作成できませんでした: {}", path.display()))?;
    file.write_all(SEED_TEMPLATE.as_bytes())
        .with_context(|| format!("シードファイルの書き込みに失敗しました: {}", path.display()))?;
    file.flush()
        .with_context(|| format!("シードファイルのクローズに失敗しました: {}", path.display()))?;

    tracing::info!("新しいシードファイルを生成しました: {}", path.display());
    tracing::info!("必要なテーブル定義とレコードを追記してください。");

    Ok(())
}

fn resolve_sequence(
    directory: &Path,
    explicit_number: Option<u32>,
    requested_width: usize,
) -> Result<(u32, usize)> {
    if let Some(number) = explicit_number {
        let width = requested_width.max(1);
        return Ok((number, width));
    }

    let mut max_number = 0u32;
    let mut detected_width = 0usize;

    let entries = fs::read_dir(directory).with_context(|| {
        format!(
            "ディレクトリを読み込めませんでした: {}",
            directory.display()
        )
    })?;

    for entry in entries {
        let entry = entry.with_context(|| {
            format!(
                "ディレクトリエントリの読み込みに失敗しました: {}",
                directory.display()
            )
        })?;
        let file_type = entry.file_type().with_context(|| {
            format!(
                "エントリの種別を取得できませんでした: {}",
                entry.path().display()
            )
        })?;
        if !file_type.is_file() {
            continue;
        }

        let path = entry.path();
        let file_name = path.file_name().and_then(|name| name.to_str());
        let Some(file_name) = file_name else {
            continue;
        };

        let digits = extract_numeric_prefix(file_name);
        if digits.is_empty() {
            continue;
        }

        detected_width = detected_width.max(digits.len());
        if let Ok(value) = digits.parse::<u32>() {
            max_number = max_number.max(value);
        }
    }

    let next = max_number.saturating_add(1);
    let width = detected_width.max(requested_width).max(3);
    Ok((next, width))
}

fn extract_numeric_prefix(name: &str) -> String {
    let mut digits = String::new();
    for ch in name.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
        } else {
            break;
        }
    }
    digits
}

fn slugify(name: &str) -> String {
    let mut slug = String::new();
    let mut last_was_hyphen = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_hyphen = false;
        } else if matches!(ch, ' ' | '-' | '_') && !last_was_hyphen && !slug.is_empty() {
            slug.push('-');
            last_was_hyphen = true;
        }
    }

    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "seed".to_string()
    } else {
        trimmed.to_string()
    }
}

const SEED_TEMPLATE: &str = r#"# yaml-seeder scaffold
version: 1
tables: []
"#;

fn collect_seed_targets(path: &Path) -> Result<Vec<PathBuf>> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("指定されたパスが存在しません: {}", path.display()))?;

    if metadata.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }

    if metadata.is_dir() {
        let mut entries = Vec::new();
        let read_dir = fs::read_dir(path)
            .with_context(|| format!("ディレクトリを読み込めませんでした: {}", path.display()))?;

        for entry in read_dir {
            let entry = entry.with_context(|| {
                format!(
                    "ディレクトリエントリの読み込みに失敗しました: {}",
                    path.display()
                )
            })?;

            let entry_path = entry.path();
            if !entry
                .file_type()
                .with_context(|| {
                    format!(
                        "エントリの種別を取得できませんでした: {}",
                        entry_path.display()
                    )
                })?
                .is_file()
            {
                continue;
            }

            let extension = entry_path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.to_ascii_lowercase());
            if matches!(extension.as_deref(), Some("yaml") | Some("yml")) {
                entries.push(entry_path);
            }
        }

        entries.sort();
        return Ok(entries);
    }

    bail!(
        "指定されたパスはファイルでもディレクトリでもありません: {}",
        path.display()
    );
}

fn load_seed_file(path: &Path) -> Result<SeedFile> {
    let file = File::open(path)
        .with_context(|| format!("YAMLファイルを開けませんでした: {}", path.display()))?;
    let reader = BufReader::new(file);
    let seed_file: SeedFile = serde_yaml::from_reader(reader)
        .with_context(|| format!("YAMLファイルの読み込みに失敗しました: {}", path.display()))?;

    seed_file
        .ensure_supported_version()
        .map_err(|err| anyhow::anyhow!("{} (file: {})", err, path.display()))?;

    Ok(seed_file)
}

async fn process_seed_file(
    pool: &sqlx::MySqlPool,
    dry_run: bool,
    validate_only: bool,
    ignore_missing_tables: bool,
    source: &Path,
    seed_file: SeedFile,
) -> Result<()> {
    tracing::info!("   テーブル定義 {} 件を処理します", seed_file.tables.len());

    let mut transaction = pool.begin().await.with_context(|| {
        format!(
            "トランザクションを開始できませんでした: {}",
            source.display()
        )
    })?;

    for table in &seed_file.tables {
        tracing::info!("   -> {} ({} 行)", table.name, table.rows.len());
        let outcome = apply_table(
            &mut transaction,
            table,
            dry_run,
            validate_only,
            ignore_missing_tables,
        )
        .await?;
        if dry_run || validate_only {
            for statement in outcome.statements {
                tracing::info!("      SQL: {statement}");
            }
        } else {
            tracing::info!("      実行済み SQL 件数: {}", outcome.statements.len());
        }
    }

    if dry_run || validate_only {
        transaction.rollback().await.with_context(|| {
            format!(
                "トランザクションのロールバックに失敗しました: {}",
                source.display()
            )
        })?;
    } else {
        transaction.commit().await.with_context(|| {
            format!(
                "トランザクションのコミットに失敗しました: {}",
                source.display()
            )
        })?;
    }

    tracing::info!("   {} の処理が完了しました。", source.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn slugify_converts_to_kebab_case() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify(" Already--Slugified "), "already-slugified");
        assert_eq!(slugify("スラッグ"), "seed");
    }

    #[test]
    fn extract_numeric_prefix_handles_various_inputs() {
        assert_eq!(extract_numeric_prefix("001-file.yaml"), "001");
        assert_eq!(extract_numeric_prefix("no-prefix.yaml"), "");
    }

    #[test]
    fn resolve_sequence_detects_next_number() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("001-alpha.yaml"), "dummy").unwrap();
        std::fs::write(dir.path().join("002-bravo.yaml"), "dummy").unwrap();

        let (number, width) =
            resolve_sequence(dir.path(), None, 3).expect("should resolve sequence");
        assert_eq!(number, 3);
        assert_eq!(width, 3);
    }

    #[test]
    fn collect_seed_targets_filters_and_sorts_yaml_files() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("002-bravo.yaml"), "dummy").unwrap();
        std::fs::write(dir.path().join("001-alpha.yml"), "dummy").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "ignore").unwrap();

        let targets = collect_seed_targets(dir.path()).expect("collect targets");
        assert_eq!(targets.len(), 2);
        assert!(targets[0].ends_with("001-alpha.yml"));
        assert!(targets[1].ends_with("002-bravo.yaml"));
    }
}
