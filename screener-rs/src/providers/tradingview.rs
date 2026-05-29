use crate::screeners::criteria::predicates_for;
use crate::screeners::models::ScreenRow;
use anyhow::Context;
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::collections::BTreeSet;

const DEFAULT_COLUMNS: &[&str] = &[
    "name",
    "description",
    "close",
    "change",
    "volume",
    "market_cap_basic",
    "type",
    "exchange",
];

const SETUP_SCORE_COLUMNS: &[&str] = &["EMA5", "EMA20", "EMA100", "EMA200", "RSI"];

const DETAIL_COLUMNS: &[&str] = &[
    "price_earnings_ttm",
    "return_on_equity",
    "dividend_yield_recent",
    "debt_to_equity",
    "RSI",
    "price_52_week_high",
    "average_volume_10d_calc",
    "relative_volume_10d_calc",
];

#[derive(Debug, Clone)]
pub struct TradingViewClient {
    client: Client,
}

#[derive(Debug, Clone)]
pub struct TradingViewScan {
    pub total_count: usize,
    pub rows: Vec<ScreenRow>,
}

impl TradingViewClient {
    pub fn new() -> anyhow::Result<Self> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (compatible; screener-rs/0.1)")
            .build()?;
        Ok(Self { client })
    }

    pub fn scan(
        &self,
        market: &str,
        columns: &[String],
        sort_by: &str,
        limit: usize,
    ) -> anyhow::Result<TradingViewScan> {
        let route = market_route(market)?;
        let url = format!("https://scanner.tradingview.com/{route}/scan");
        let payload = json!({
            "columns": columns,
            "filter": [{"left": "type", "operation": "equal", "right": "stock"}],
            "range": [0, limit],
            "sort": {"sortBy": sort_by, "sortOrder": "desc"},
        });
        let raw: Value = self
            .client
            .post(url)
            .json(&payload)
            .send()
            .context("TradingView scanner request failed")?
            .error_for_status()
            .context("TradingView scanner returned non-success")?
            .json()
            .context("TradingView scanner JSON parse failed")?;
        parse_scan_response(&raw, columns)
    }

    pub fn screen_rows(
        &self,
        market: &str,
        criteria_names: &[String],
        order_by: &str,
        limit: usize,
        detail: bool,
    ) -> anyhow::Result<Vec<ScreenRow>> {
        let columns = screen_columns(criteria_names, order_by, detail)?;
        let fetch_limit = if order_by == "setup_score" {
            (limit * 50).max(1000)
        } else {
            (limit * 20).max(1000)
        }
        .min(5000);
        Ok(self
            .scan(
                market,
                &columns,
                if order_by == "setup_score" {
                    "volume"
                } else {
                    order_by
                },
                fetch_limit,
            )?
            .rows)
    }

    pub fn liquid_universe(
        &self,
        market: &str,
        limit: usize,
        min_close: f64,
        min_market_cap: Option<f64>,
    ) -> anyhow::Result<Vec<ScreenRow>> {
        let columns = DEFAULT_COLUMNS
            .iter()
            .copied()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let fetch_limit = (limit * 4).max(limit).clamp(500, 5000);
        let mut rows = self.scan(market, &columns, "volume", fetch_limit)?.rows;
        rows.retain(|row| {
            let exchange = row.text("exchange").unwrap_or_default();
            let exchange_ok = match market {
                "india" => matches!(exchange.as_str(), "NSE" | "BSE"),
                "us" => matches!(exchange.as_str(), "NASDAQ" | "NYSE" | "AMEX"),
                _ => true,
            };
            let cap_ok = min_market_cap
                .map(|floor| row.numeric("market_cap_basic").unwrap_or(0.0) >= floor)
                .unwrap_or(true);
            exchange_ok
                && row.text("type").as_deref() == Some("stock")
                && row.numeric("close").unwrap_or(0.0) >= min_close
                && row.numeric("volume").unwrap_or(0.0) >= 1000.0
                && cap_ok
        });
        dedupe_listings(&mut rows);
        rows.truncate(limit);
        Ok(rows)
    }
}

pub fn screen_columns(
    criteria_names: &[String],
    order_by: &str,
    detail: bool,
) -> anyhow::Result<Vec<String>> {
    let mut set = DEFAULT_COLUMNS
        .iter()
        .copied()
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    if detail {
        set.extend(DETAIL_COLUMNS.iter().copied().map(str::to_string));
    }
    if order_by == "setup_score" {
        set.extend(SETUP_SCORE_COLUMNS.iter().copied().map(str::to_string));
    } else {
        set.insert(order_by.to_string());
    }
    for name in criteria_names {
        for predicate in predicates_for(name)? {
            set.extend(predicate.fields().into_iter().map(str::to_string));
        }
    }
    Ok(set.into_iter().collect())
}

pub fn parse_scan_response(raw: &Value, columns: &[String]) -> anyhow::Result<TradingViewScan> {
    let total_count = raw.get("totalCount").and_then(Value::as_u64).unwrap_or(0) as usize;
    let mut rows = Vec::new();
    for item in raw
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut row = ScreenRow::default();
        if let Some(ticker) = item.get("s").and_then(Value::as_str) {
            row.ticker = Some(ticker.to_string());
        }
        let Some(values) = item.get("d").and_then(Value::as_array) else {
            continue;
        };
        for (column, value) in columns.iter().zip(values.iter()) {
            if value.is_null() {
                continue;
            }
            match column.as_str() {
                "name" => {
                    if let Some(text) = value.as_str() {
                        row.name = Some(text.to_string());
                    }
                }
                "description" => {
                    if let Some(text) = value.as_str() {
                        row.description = Some(text.to_string());
                    }
                }
                field => {
                    if let Some(n) = value.as_f64() {
                        row.set_numeric(field, n);
                    } else if let Some(s) = value.as_str() {
                        row.set_text(field, s);
                    } else if let Some(b) = value.as_bool() {
                        row.fields
                            .insert(field.to_string(), serde_yaml::Value::from(b));
                    }
                }
            }
        }
        rows.push(row);
    }
    Ok(TradingViewScan { total_count, rows })
}

fn market_route(market: &str) -> anyhow::Result<&'static str> {
    match market {
        "us" => Ok("america"),
        "india" => Ok("india"),
        _ => anyhow::bail!("unknown TradingView market {market:?}"),
    }
}

fn dedupe_listings(rows: &mut Vec<ScreenRow>) {
    let mut seen = BTreeSet::new();
    rows.retain(|row| {
        let raw = row
            .description
            .as_deref()
            .or(row.name.as_deref())
            .or(row.ticker.as_deref())
            .unwrap_or_default();
        let key = raw
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>();
        seen.insert(key)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scanner_rows() {
        let columns = vec![
            "name".to_string(),
            "description".to_string(),
            "close".to_string(),
            "exchange".to_string(),
        ];
        let raw = serde_json::json!({
            "totalCount": 1,
            "data": [{"s": "NASDAQ:ABC", "d": ["ABC", "ABC Corp", 12.5, "NASDAQ"]}]
        });
        let parsed = parse_scan_response(&raw, &columns).unwrap();
        assert_eq!(parsed.total_count, 1);
        assert_eq!(parsed.rows[0].ticker.as_deref(), Some("NASDAQ:ABC"));
        assert_eq!(parsed.rows[0].name.as_deref(), Some("ABC"));
        assert_eq!(parsed.rows[0].numeric("close"), Some(12.5));
        assert_eq!(parsed.rows[0].text("exchange").as_deref(), Some("NASDAQ"));
    }
}
