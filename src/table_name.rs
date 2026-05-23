use anyhow::{Result, anyhow};

/// TODO: add English documentation
#[derive(Debug, Clone)]
pub struct TableName {
    schema: String,
    table: String,
}

impl TableName {
    /// TODO: add English documentation
    pub fn parse(input: &str) -> Result<Self> {
        let mut parts = input.split('.');
        let schema = parts
            .next()
            .ok_or_else(|| anyhow!("table name must include schema"))?
            .trim();
        let table = parts
            .next()
            .ok_or_else(|| anyhow!("table name must include table segment"))?
            .trim();
        if parts.next().is_some() {
            return Err(anyhow!("table name must have exactly one dot"));
        }
        if schema.is_empty() || table.is_empty() {
            return Err(anyhow!("schema and table segments must be non-empty"));
        }
        Ok(Self {
            schema: schema.to_string(),
            table: table.to_string(),
        })
    }

    /// TODO: add English documentation
    pub fn schema(&self) -> &str {
        &self.schema
    }

    /// TODO: add English documentation
    pub fn table(&self) -> &str {
        &self.table
    }

    /// TODO: add English documentation
    pub fn quoted(&self) -> String {
        format!("`{}`.`{}`", self.schema, self.table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_schema_table() {
        let name = TableName::parse("foo.bar").expect("parseable");
        assert_eq!(name.schema(), "foo");
        assert_eq!(name.table(), "bar");
        assert_eq!(name.quoted(), "`foo`.`bar`");
    }

    #[test]
    fn parse_with_whitespace_is_trimmed() {
        let name = TableName::parse(" foo . bar ").expect("parseable");
        assert_eq!(name.schema(), "foo");
        assert_eq!(name.table(), "bar");
    }

    #[test]
    fn parse_rejects_missing_segments() {
        assert!(TableName::parse("only_schema").is_err());
        assert!(TableName::parse("schema.").is_err());
    }

    #[test]
    fn parse_rejects_additional_segments() {
        assert!(TableName::parse("a.b.c").is_err());
    }
}
