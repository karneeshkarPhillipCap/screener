use crate::screeners::criteria::{pipeline_names, predicates_for};
use crate::screeners::models::ScreenRow;
use std::cmp::Ordering;

#[derive(Debug, Clone)]
pub struct ScreenRequest {
    pub market: String,
    pub criteria_names: Vec<String>,
    pub limit: usize,
    pub order_by: String,
    pub detail: bool,
}

pub fn screen_rows(rows: &[ScreenRow], request: &ScreenRequest) -> anyhow::Result<Vec<ScreenRow>> {
    let pipeline = request
        .criteria_names
        .iter()
        .find(|name| pipeline_names().contains(name.as_str()));
    if let Some(name) = pipeline {
        anyhow::bail!(
            "pipeline criterion {name:?} needs a dedicated provider-backed Rust command; CSV screen supports column criteria only"
        );
    }

    let mut predicates = Vec::new();
    for name in &request.criteria_names {
        predicates.extend(predicates_for(name)?);
    }
    let mut out = rows
        .iter()
        .filter(|row| predicates.iter().all(|predicate| predicate.evaluate(row)))
        .cloned()
        .collect::<Vec<_>>();

    if request.order_by == "setup_score" {
        for row in &mut out {
            let score = setup_score(row);
            row.set_numeric("setup_score", (score * 100.0).round() / 100.0);
        }
    }

    out.sort_by(|a, b| compare_rows(a, b, &request.order_by));
    out.truncate(request.limit);
    if !request.detail {
        hide_non_detail_columns(&mut out);
    }
    Ok(out)
}

fn compare_rows(a: &ScreenRow, b: &ScreenRow, field: &str) -> Ordering {
    let av = a.numeric(field).unwrap_or(f64::NEG_INFINITY);
    let bv = b.numeric(field).unwrap_or(f64::NEG_INFINITY);
    bv.partial_cmp(&av).unwrap_or(Ordering::Equal)
}

fn hide_non_detail_columns(rows: &mut [ScreenRow]) {
    for row in rows {
        for field in ["EMA5", "EMA20", "EMA100", "EMA200"] {
            row.fields.remove(field);
        }
    }
}

fn setup_score(row: &ScreenRow) -> f64 {
    let close = row.numeric("close").unwrap_or(0.0);
    let ema5 = row.numeric("EMA5").unwrap_or(0.0);
    let ema20 = row.numeric("EMA20").unwrap_or(0.0);
    let ema100 = row.numeric("EMA100").unwrap_or(0.0);
    let ema200 = row.numeric("EMA200").unwrap_or(0.0);
    let change = row.numeric("change").unwrap_or(0.0);
    let rsi = row.numeric("RSI").unwrap_or(0.0);
    let volume = row.numeric("volume").unwrap_or(0.0);
    let market_cap = row.numeric("market_cap_basic").unwrap_or(0.0);

    let liquidity = log_score(close * volume, 1_000_000.0, 10_000_000_000.0);
    let market_cap_score = log_score(market_cap, 100_000_000.0, 1_000_000_000_000.0);
    let trend_spread = if close > 0.0 && ema20 > 0.0 && ema100 > 0.0 && ema200 > 0.0 {
        ((ema5 - ema20) / close + (ema20 - ema100) / close + (ema100 - ema200) / close)
            .clamp(0.0, 0.35)
            / 0.35
    } else {
        0.0
    };
    let momentum = ((change.clamp(-5.0, 10.0) + 5.0) / 15.0).clamp(0.0, 1.0);
    let rsi_quality = (1.0 - ((rsi - 60.0).abs() / 40.0)).clamp(0.0, 1.0);
    let price_quality = (close.clamp(0.0, 200.0) / 200.0).clamp(0.0, 1.0);
    let extension = if ema20 != 0.0 {
        (close - ema20) / ema20
    } else {
        0.0
    };
    let overextension_penalty = ((extension - 0.12).max(0.0) / 0.25).clamp(0.0, 1.0);

    25.0 * liquidity
        + 30.0 * trend_spread
        + 15.0 * momentum
        + 15.0 * market_cap_score
        + 10.0 * rsi_quality
        + 5.0 * price_quality
        - 15.0 * overextension_penalty
}

fn log_score(value: f64, low: f64, high: f64) -> f64 {
    if value <= low {
        return 0.0;
    }
    if value >= high {
        return 1.0;
    }
    ((value.ln() - low.ln()) / (high.ln() - low.ln())).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screeners::models::ScreenRow;
    use std::collections::BTreeMap;

    fn row(name: &str, close: f64, ema5: f64, ema20: f64, ema100: f64, ema200: f64) -> ScreenRow {
        let mut fields = BTreeMap::new();
        for (k, v) in [
            ("close", close),
            ("EMA5", ema5),
            ("EMA20", ema20),
            ("EMA100", ema100),
            ("EMA200", ema200),
            ("volume", 1_000_000.0),
            ("market_cap_basic", 10_000_000_000.0),
            ("change", 2.0),
            ("RSI", 60.0),
        ] {
            fields.insert(k.to_string(), serde_yaml::Value::from(v));
        }
        ScreenRow {
            ticker: Some(name.to_string()),
            name: Some(name.to_string()),
            description: None,
            fields,
        }
    }

    #[test]
    fn ema_screen_filters_and_scores() {
        let rows = vec![
            row("PASS", 100.0, 105.0, 95.0, 80.0, 70.0),
            row("FAIL", 100.0, 90.0, 95.0, 80.0, 70.0),
        ];
        let out = screen_rows(
            &rows,
            &ScreenRequest {
                market: "us".to_string(),
                criteria_names: vec!["ema".to_string()],
                limit: 10,
                order_by: "setup_score".to_string(),
                detail: false,
            },
        )
        .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name.as_deref(), Some("PASS"));
        assert!(out[0].numeric("setup_score").unwrap() > 0.0);
        assert!(out[0].numeric("EMA5").is_none());
    }
}
