use crate::backtester::PriceFetcher;
use crate::data::{Bar, Bars, PricePanel};
use anyhow::Context;
use chrono::{Duration, NaiveDate};
use reqwest::blocking::Client;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct YahooPriceFetcher {
    client: Client,
}

impl YahooPriceFetcher {
    pub fn new() -> anyhow::Result<Self> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (compatible; screener-rs/0.1)")
            .build()?;
        Ok(Self { client })
    }

    pub fn fetch_symbol(
        &self,
        symbol: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Bars> {
        let period1 = date_ts(start)?;
        let period2 = date_ts(end + Duration::days(1))?;
        let encoded = urlencoding::encode(symbol);
        let url = format!(
            "https://query1.finance.yahoo.com/v8/finance/chart/{encoded}?period1={period1}&period2={period2}&interval=1d&events=div%2Csplits&includeAdjustedClose=true"
        );
        let payload: Value = self
            .client
            .get(url)
            .send()
            .with_context(|| format!("Yahoo chart request failed for {symbol}"))?
            .error_for_status()
            .with_context(|| format!("Yahoo chart returned non-success for {symbol}"))?
            .json()
            .with_context(|| format!("Yahoo chart JSON parse failed for {symbol}"))?;
        parse_chart_payload(&payload)
            .with_context(|| format!("invalid Yahoo chart payload for {symbol}"))
    }
}

impl PriceFetcher for YahooPriceFetcher {
    fn fetch(
        &self,
        tickers: &[String],
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<PricePanel> {
        let mut out = PricePanel::new();
        for ticker in tickers {
            match self.fetch_symbol(ticker, start, end) {
                Ok(bars) => {
                    out.insert(ticker.clone(), bars);
                }
                Err(_) => {
                    out.insert(ticker.clone(), Bars::default());
                }
            }
        }
        Ok(out)
    }
}

fn date_ts(day: NaiveDate) -> anyhow::Result<i64> {
    Ok(day
        .and_hms_opt(0, 0, 0)
        .context("invalid date")?
        .and_utc()
        .timestamp())
}

fn parse_chart_payload(payload: &Value) -> anyhow::Result<Bars> {
    let result = payload
        .pointer("/chart/result/0")
        .context("missing chart result")?;
    let timestamps = result
        .get("timestamp")
        .and_then(Value::as_array)
        .context("missing timestamp array")?;
    let quote = result
        .pointer("/indicators/quote/0")
        .context("missing quote indicators")?;
    let open = json_array(quote, "open")?;
    let high = json_array(quote, "high")?;
    let low = json_array(quote, "low")?;
    let close = json_array(quote, "close")?;
    let volume = json_array(quote, "volume")?;
    let adj_close = result
        .pointer("/indicators/adjclose/0/adjclose")
        .and_then(Value::as_array);
    let dividends = dividend_map(result);

    let mut rows = Vec::new();
    for (i, ts) in timestamps.iter().enumerate() {
        let Some(ts) = ts.as_i64() else {
            continue;
        };
        let Some(date) = chrono::DateTime::from_timestamp(ts, 0).map(|dt| dt.date_naive()) else {
            continue;
        };
        let (Some(open), Some(high), Some(low), Some(close), Some(volume)) = (
            array_f64(open, i),
            array_f64(high, i),
            array_f64(low, i),
            array_f64(close, i),
            array_f64(volume, i),
        ) else {
            continue;
        };
        rows.push(Bar {
            date,
            open,
            high,
            low,
            close,
            volume,
            adj_close: adj_close.and_then(|arr| array_f64(arr, i)),
            dividend: dividends.get(&date).copied(),
        });
    }
    Ok(Bars::new(rows))
}

fn json_array<'a>(value: &'a Value, key: &str) -> anyhow::Result<&'a Vec<Value>> {
    value
        .get(key)
        .and_then(Value::as_array)
        .with_context(|| format!("missing {key} array"))
}

fn array_f64(values: &[Value], idx: usize) -> Option<f64> {
    let out = values.get(idx)?.as_f64()?;
    out.is_finite().then_some(out)
}

fn dividend_map(result: &Value) -> BTreeMap<NaiveDate, f64> {
    let mut out = BTreeMap::new();
    let Some(divs) = result
        .pointer("/events/dividends")
        .and_then(Value::as_object)
    else {
        return out;
    };
    for value in divs.values() {
        let Some(ts) = value.get("date").and_then(Value::as_i64) else {
            continue;
        };
        let Some(amount) = value.get("amount").and_then(Value::as_f64) else {
            continue;
        };
        if let Some(day) = chrono::DateTime::from_timestamp(ts, 0).map(|dt| dt.date_naive()) {
            *out.entry(day).or_insert(0.0) += amount;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chart_payload() {
        let payload = serde_json::json!({
            "chart": {"result": [{
                "timestamp": [1714521600],
                "indicators": {
                    "quote": [{"open": [1.0], "high": [2.0], "low": [0.5], "close": [1.5], "volume": [1000]}],
                    "adjclose": [{"adjclose": [1.4]}]
                },
                "events": {"dividends": {"a": {"date": 1714521600, "amount": 0.1}}}
            }]}
        });
        let bars = parse_chart_payload(&payload).unwrap();
        assert_eq!(bars.len(), 1);
        assert_eq!(bars.rows[0].close, 1.5);
        assert_eq!(bars.rows[0].adj_close, Some(1.4));
        assert_eq!(bars.rows[0].dividend, Some(0.1));
    }
}
