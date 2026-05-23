use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_yaml::Value;

/// TODO: add English documentation
#[derive(Debug, Deserialize, Serialize)]
pub struct SeedFile {
    /// TODO: add English documentation
    pub version: u8,
    /// TODO: add English documentation
    #[serde(default)]
    pub tables: Vec<TableSeed>,
}

/// TODO: add English documentation
#[derive(Debug, Deserialize, Serialize)]
pub struct TableSeed {
    /// TODO: add English documentation
    pub name: String,
    /// TODO: add English documentation
    #[serde(default = "TableMode::default")]
    pub mode: TableMode,
    /// TODO: add English documentation
    #[serde(default)]
    pub update_columns: Option<Vec<String>>,
    /// TODO: add English documentation
    #[serde(default)]
    pub rows: Vec<TableRow>,
}

/// TODO: add English documentation
pub type TableRow = BTreeMap<String, Value>;

/// TODO: add English documentation
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TableMode {
    /// TODO: add English documentation
    Insert,
    /// TODO: add English documentation
    UpsertUpdate,
}

impl TableMode {
    /// TODO: add English documentation
    pub const fn default() -> Self {
        Self::UpsertUpdate
    }
}

impl SeedFile {
    /// TODO: add English documentation
    pub fn ensure_supported_version(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(format!("unsupported seed file version: {}", self.version));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_mode_default_is_upsert_update() {
        assert_eq!(TableMode::default(), TableMode::UpsertUpdate);
    }

    #[test]
    fn ensure_supported_version_accepts_one() {
        let seed = SeedFile {
            version: 1,
            tables: Vec::new(),
        };
        assert!(seed.ensure_supported_version().is_ok());
    }

    #[test]
    fn ensure_supported_version_rejects_other_versions() {
        let seed = SeedFile {
            version: 2,
            tables: Vec::new(),
        };
        let err = seed.ensure_supported_version().unwrap_err();
        assert!(err.contains("unsupported seed file version"));
    }
}
