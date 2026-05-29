use crate::screeners::unusual_volume::UnusualVolumeEvent;
use anyhow::Context;
use regex::Regex;
use reqwest::blocking::Client;
use serde_json::Value;

const NSE_PLEDGE_URL: &str = "https://www.nseindia.com/api/corporate-pledgedata?symbol=";

pub fn resolve_pledge_pct(symbol: &str) -> anyhow::Result<Option<f64>> {
    let client = Client::builder()
        .user_agent("Mozilla/5.0 (compatible; screener-rs/0.1)")
        .build()?;
    if let Some(value) = fetch_nse_pledge_pct(&client, symbol).unwrap_or(None) {
        return Ok(Some(value));
    }
    Ok(fetch_openscreener_pledge_pct(&client, symbol).unwrap_or(None))
}

pub fn overlay_pledge(events: &mut [UnusualVolumeEvent]) -> anyhow::Result<()> {
    for event in events {
        if let Some(value) = resolve_pledge_pct(&event.symbol)? {
            event.pledge_pct = Some(value);
        }
    }
    Ok(())
}

fn fetch_nse_pledge_pct(client: &Client, symbol: &str) -> anyhow::Result<Option<f64>> {
    let url = format!(
        "{}{}",
        NSE_PLEDGE_URL,
        urlencoding::encode(&symbol.trim().to_uppercase())
    );
    let raw: Value = client
        .get(url)
        .send()
        .with_context(|| format!("NSE pledge request failed for {symbol}"))?
        .error_for_status()
        .with_context(|| format!("NSE pledge returned non-success for {symbol}"))?
        .json()
        .context("NSE pledge JSON parse failed")?;
    Ok(parse_nse_pledge_pct(&raw))
}

fn fetch_openscreener_pledge_pct(client: &Client, symbol: &str) -> anyhow::Result<Option<f64>> {
    let url = format!(
        "https://www.screener.in/company/{}/",
        urlencoding::encode(&symbol.trim().to_uppercase())
    );
    let html = client
        .get(url)
        .send()
        .with_context(|| format!("screener.in pledge request failed for {symbol}"))?
        .error_for_status()
        .with_context(|| format!("screener.in pledge returned non-success for {symbol}"))?
        .text()
        .context("failed reading screener.in pledge HTML")?;
    Ok(parse_openscreener_pledge_pct(&html))
}

pub fn parse_nse_pledge_pct(raw: &Value) -> Option<f64> {
    let rows = raw
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| raw.as_array())?;
    let latest = rows.first()?.as_object()?;
    for key in [
        "per. of Promoter Holding Shares pledge",
        "percentageOfPromoterHoldingPledged",
        "pledgePercentage",
        "perShareEncumbered",
    ] {
        if let Some(value) = latest.get(key).and_then(as_pct) {
            return Some(value);
        }
    }
    latest.iter().find_map(|(key, value)| {
        let key = key.to_ascii_lowercase();
        (key.contains("pledge")
            && (key.contains("per") || key.contains('%') || key.contains("percent")))
        .then(|| as_pct(value))
        .flatten()
    })
}

pub fn parse_openscreener_pledge_pct(html: &str) -> Option<f64> {
    let re = Regex::new(r"(?is)pledged?\s*percentage[^0-9%]{0,60}?([0-9][0-9,]*(?:\.[0-9]+)?)\s*%")
        .ok()?;
    re.captures(html)
        .and_then(|caps| caps.get(1))
        .and_then(|m| parse_pct_text(m.as_str()))
}

fn as_pct(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64().and_then(validate_pct),
        Value::String(s) => parse_pct_text(s),
        _ => None,
    }
}

fn parse_pct_text(raw: &str) -> Option<f64> {
    raw.replace([',', '%'], "")
        .trim()
        .parse::<f64>()
        .ok()
        .and_then(validate_pct)
}

fn validate_pct(value: f64) -> Option<f64> {
    (value.is_finite() && (0.0..=100.0).contains(&value)).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nse_pledge_payload() {
        let raw = serde_json::json!({
            "data": [{"percentageOfPromoterHoldingPledged": "12.34%"}]
        });
        assert_eq!(parse_nse_pledge_pct(&raw), Some(12.34));
    }

    #[test]
    fn parses_openscreener_pledge_html() {
        let html = "<span>Pledged percentage</span><span>7.50%</span>";
        assert_eq!(parse_openscreener_pledge_pct(html), Some(7.5));
    }
}
