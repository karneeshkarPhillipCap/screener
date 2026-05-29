use crate::backtester::models::{BacktestConfig, BacktestResult};
use crate::backtester::{PriceFetcher, run_rolling_backtest};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ParameterValue {
    None,
    Float(f64),
    Int(usize),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GridResult {
    pub params: BTreeMap<String, ParameterValue>,
    pub score: f64,
    pub metrics: BTreeMap<String, f64>,
    pub trade_count: usize,
}

pub fn parameter_combinations(
    grid: &BTreeMap<String, Vec<ParameterValue>>,
) -> Vec<BTreeMap<String, ParameterValue>> {
    let mut keys = grid.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    let mut out = Vec::new();
    let mut current = BTreeMap::new();
    fn walk(
        idx: usize,
        keys: &[String],
        grid: &BTreeMap<String, Vec<ParameterValue>>,
        current: &mut BTreeMap<String, ParameterValue>,
        out: &mut Vec<BTreeMap<String, ParameterValue>>,
    ) {
        if idx == keys.len() {
            out.push(current.clone());
            return;
        }
        let key = &keys[idx];
        if let Some(values) = grid.get(key) {
            for value in values {
                current.insert(key.clone(), value.clone());
                walk(idx + 1, keys, grid, current, out);
            }
            current.remove(key);
        }
    }
    walk(0, &keys, grid, &mut current, &mut out);
    out
}

pub fn grid_search(
    base: &BacktestConfig,
    fetcher: &dyn PriceFetcher,
    grid: &BTreeMap<String, Vec<ParameterValue>>,
    metric: &str,
    min_trades: usize,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> anyhow::Result<Vec<GridResult>> {
    let mut results = Vec::new();
    for params in parameter_combinations(grid) {
        let cfg = apply_params(base, &params)?;
        let result = run_rolling_backtest(cfg, fetcher, start_date, end_date)?;
        let score = score_result(&result, metric, min_trades);
        results.push(GridResult {
            params,
            score,
            metrics: result.metrics,
            trade_count: result.trades.len(),
        });
    }
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(results)
}

fn apply_params(
    base: &BacktestConfig,
    params: &BTreeMap<String, ParameterValue>,
) -> anyhow::Result<BacktestConfig> {
    let mut cfg = base.clone();
    for (key, value) in params {
        match (key.as_str(), value) {
            ("stop_loss", ParameterValue::None) => cfg.stop_loss = None,
            ("stop_loss", ParameterValue::Float(v)) => cfg.stop_loss = Some(*v),
            ("take_profit", ParameterValue::None) => cfg.take_profit = None,
            ("take_profit", ParameterValue::Float(v)) => cfg.take_profit = Some(*v),
            ("trailing_stop", ParameterValue::None) => cfg.trailing_stop = None,
            ("trailing_stop", ParameterValue::Float(v)) => cfg.trailing_stop = Some(*v),
            ("hold", ParameterValue::Int(v)) => cfg.hold = *v,
            _ => anyhow::bail!("unsupported grid parameter {key:?}={value:?}"),
        }
    }
    Ok(cfg)
}

fn score_result(result: &BacktestResult, metric: &str, min_trades: usize) -> f64 {
    if result.trades.len() < min_trades {
        return f64::NEG_INFINITY;
    }
    let score = result
        .metrics
        .get(metric)
        .copied()
        .unwrap_or(f64::NEG_INFINITY);
    if score.is_finite() {
        score
    } else {
        f64::NEG_INFINITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_combinations_count() {
        let grid = BTreeMap::from([
            (
                "stop_loss".to_string(),
                vec![ParameterValue::None, ParameterValue::Float(0.05)],
            ),
            (
                "take_profit".to_string(),
                vec![
                    ParameterValue::Float(0.1),
                    ParameterValue::Float(0.2),
                    ParameterValue::Float(0.3),
                ],
            ),
            (
                "hold".to_string(),
                vec![ParameterValue::Int(10), ParameterValue::Int(20)],
            ),
        ]);
        assert_eq!(parameter_combinations(&grid).len(), 12);
    }
}
