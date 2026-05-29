use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ExpressionAlias {
    pub entry: String,
    #[serde(default)]
    pub exit: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CliConfig {
    #[serde(default)]
    pub strategies: BTreeMap<String, ExpressionAlias>,
    #[serde(default)]
    pub criteria: BTreeMap<String, ExpressionAlias>,
}

pub fn load_yaml_config(path: impl AsRef<Path>) -> anyhow::Result<CliConfig> {
    let text = fs::read_to_string(path)?;
    let cfg = serde_yaml::from_str(&text)?;
    Ok(cfg)
}

pub fn built_in_strategy(name: &str) -> Option<ExpressionAlias> {
    match name {
        "ma_cross" => Some(ExpressionAlias {
            entry: "crossover(sma(close, 20), sma(close, 50))".to_string(),
            exit: Some("crossunder(sma(close, 20), sma(close, 50))".to_string()),
            description: Some("20/50 SMA crossover".to_string()),
        }),
        "ema_trend" => Some(ExpressionAlias {
            entry: "close > ema(close, 20) and ema(close, 20) > ema(close, 50)".to_string(),
            exit: Some("close < ema(close, 20)".to_string()),
            description: Some("EMA trend-following baseline".to_string()),
        }),
        _ => None,
    }
}
