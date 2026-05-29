use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ScreenRow {
    #[serde(default)]
    pub ticker: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(flatten)]
    pub fields: BTreeMap<String, serde_yaml::Value>,
}

impl ScreenRow {
    pub fn numeric(&self, field: &str) -> Option<f64> {
        match field {
            "ticker" | "name" | "description" => None,
            _ => self.fields.get(field).and_then(value_to_f64),
        }
    }

    pub fn text(&self, field: &str) -> Option<String> {
        match field {
            "ticker" => self.ticker.clone(),
            "name" => self.name.clone(),
            "description" => self.description.clone(),
            _ => self.fields.get(field).and_then(value_to_string),
        }
    }

    pub fn set_numeric(&mut self, field: &str, value: f64) {
        self.fields
            .insert(field.to_string(), serde_yaml::Value::from(value));
    }

    pub fn set_text(&mut self, field: &str, value: impl Into<String>) {
        let value = value.into();
        match field {
            "ticker" => self.ticker = Some(value),
            "name" => self.name = Some(value),
            "description" => self.description = Some(value),
            _ => {
                self.fields
                    .insert(field.to_string(), serde_yaml::Value::from(value));
            }
        }
    }

    pub fn display_columns(&self) -> Vec<String> {
        let mut cols = Vec::new();
        for name in ["ticker", "name", "description"] {
            if self.text(name).is_some() {
                cols.push(name.to_string());
            }
        }
        cols.extend(self.fields.keys().cloned());
        cols
    }
}

pub fn value_to_f64(value: &serde_yaml::Value) -> Option<f64> {
    match value {
        serde_yaml::Value::Number(n) => n.as_f64(),
        serde_yaml::Value::String(s) => s.parse().ok(),
        serde_yaml::Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

pub fn value_to_string(value: &serde_yaml::Value) -> Option<String> {
    match value {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        serde_yaml::Value::Null => None,
        other => Some(format!("{other:?}")),
    }
}

pub fn read_screen_csv(path: impl AsRef<Path>) -> anyhow::Result<Vec<ScreenRow>> {
    let mut rdr = csv::Reader::from_path(path)?;
    let headers = rdr.headers()?.clone();
    let mut rows = Vec::new();
    for record in rdr.records() {
        let record = record?;
        let mut row = ScreenRow::default();
        for (header, raw) in headers.iter().zip(record.iter()) {
            if raw.trim().is_empty() {
                continue;
            }
            match header {
                "ticker" => row.ticker = Some(raw.to_string()),
                "name" => row.name = Some(raw.to_string()),
                "description" => row.description = Some(raw.to_string()),
                field => {
                    let value = raw
                        .parse::<f64>()
                        .map(serde_yaml::Value::from)
                        .unwrap_or_else(|_| serde_yaml::Value::from(raw.to_string()));
                    row.fields.insert(field.to_string(), value);
                }
            }
        }
        rows.push(row);
    }
    Ok(rows)
}

pub fn write_screen_csv(rows: &[ScreenRow]) -> anyhow::Result<()> {
    let mut columns = Vec::<String>::new();
    for row in rows {
        for column in row.display_columns() {
            if !columns.contains(&column) {
                columns.push(column);
            }
        }
    }
    let mut writer = csv::Writer::from_writer(std::io::stdout());
    writer.write_record(&columns)?;
    for row in rows {
        let values = columns
            .iter()
            .map(|column| {
                row.text(column).unwrap_or_else(|| {
                    row.numeric(column)
                        .map(|n| n.to_string())
                        .unwrap_or_default()
                })
            })
            .collect::<Vec<_>>();
        writer.write_record(values)?;
    }
    writer.flush()?;
    Ok(())
}
