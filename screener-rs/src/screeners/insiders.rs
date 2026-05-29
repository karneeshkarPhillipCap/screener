use crate::providers::screener_in::ScreenerInClient;
use crate::screeners::models::ScreenRow;
use anyhow::Context;
use chrono::{Duration, NaiveDate, Utc};
use rayon::prelude::*;
use reqwest::blocking::Client;
use serde_json::Value;

const FMP_WINDOW_DAYS: i64 = 182;
const FMP_MAX_PAGES: usize = 10;

#[derive(Debug, Clone)]
pub struct PromoterBuyRequest {
    pub market: String,
    pub limit: usize,
    pub min_change_pct: f64,
    pub min_yf_net_pct: Option<f64>,
    pub require_both: bool,
}

#[derive(Debug, Clone, Default)]
struct FmpAggregate {
    net_shares_6m: f64,
    buy_shares_6m: f64,
    sell_shares_6m: f64,
    buy_trans_6m: usize,
    sell_trans_6m: usize,
}

pub fn screen_promoter_buys(
    universe: &[ScreenRow],
    request: &PromoterBuyRequest,
) -> anyhow::Result<Vec<ScreenRow>> {
    match request.market.as_str() {
        "india" => screen_india_promoters(universe, request),
        "us" => screen_us_insiders(universe, request),
        other => anyhow::bail!("unknown promoter-buys market {other:?}"),
    }
}

fn screen_india_promoters(
    universe: &[ScreenRow],
    request: &PromoterBuyRequest,
) -> anyhow::Result<Vec<ScreenRow>> {
    if request.require_both {
        anyhow::bail!(
            "Rust promoter-buys supports screener.in promoter deltas; --require-both needs Yahoo insider-purchases, which is not exposed by the Rust provider yet"
        );
    }
    if request.min_yf_net_pct.is_some() {
        anyhow::bail!(
            "--min-yf-net-pct is US/yfinance-only and is not supported by Rust promoter-buys"
        );
    }
    let client = ScreenerInClient::new()?;
    let mut out = Vec::new();
    let bases = universe
        .iter()
        .filter_map(|base| {
            base.text("name")
                .or_else(|| base.text("ticker"))
                .map(|name| (base, india_symbol(&name)))
        })
        .collect::<Vec<_>>();
    let symbols = bases
        .iter()
        .map(|(_, symbol)| symbol.clone())
        .collect::<Vec<_>>();
    let snapshots = client.promoter_snapshots(&symbols);
    for ((base, symbol), (_, snapshot)) in bases.into_iter().zip(snapshots) {
        let Some(snapshot) = snapshot else {
            continue;
        };
        if snapshot.promoter_change <= request.min_change_pct {
            continue;
        }
        let mut row = base.clone();
        row.name = Some(symbol);
        row.set_numeric("promoter_pct_latest", snapshot.promoter_pct_latest);
        row.set_numeric("promoter_pct_prev", snapshot.promoter_pct_prev);
        row.set_numeric("promoter_change", snapshot.promoter_change);
        if let Some(q) = snapshot.latest_quarter {
            row.set_text("latest_quarter", q);
        }
        if let Some(v) = snapshot.fii_pct_latest {
            row.set_numeric("fii_pct_latest", v);
        }
        if let Some(v) = snapshot.dii_pct_latest {
            row.set_numeric("dii_pct_latest", v);
        }
        out.push(row);
    }
    out.sort_by(|a, b| {
        b.numeric("promoter_change")
            .unwrap_or(f64::NEG_INFINITY)
            .partial_cmp(&a.numeric("promoter_change").unwrap_or(f64::NEG_INFINITY))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out.truncate(request.limit);
    Ok(out)
}

fn screen_us_insiders(
    universe: &[ScreenRow],
    request: &PromoterBuyRequest,
) -> anyhow::Result<Vec<ScreenRow>> {
    let Some(api_key) = std::env::var("FMP_API_KEY")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        anyhow::bail!(
            "Rust promoter-buys for US requires FMP_API_KEY; the Python yfinance insider-purchases fallback is not cloned in Rust"
        );
    };
    let client = Client::builder()
        .user_agent("Mozilla/5.0 (compatible; screener-rs/0.1)")
        .build()?;
    let cutoff = Utc::now().date_naive() - Duration::days(FMP_WINDOW_DAYS);
    if request.min_yf_net_pct.is_some() {
        anyhow::bail!(
            "--min-yf-net-pct depends on Yahoo insider-purchases percentages; Rust US promoter-buys currently uses FMP Form 4 share counts"
        );
    }
    let mut out = universe
        .par_iter()
        .filter_map(|base| {
            let symbol = base
                .text("name")
                .or_else(|| base.text("ticker"))
                .map(|s| bare_symbol(&s))?;
            let agg = fetch_fmp_aggregate(&client, &symbol, &api_key, cutoff).ok()??;
            if agg.net_shares_6m <= 0.0 {
                return None;
            }
            let mut row = base.clone();
            row.name = Some(symbol);
            row.set_numeric("fmp_net_shares_6m", agg.net_shares_6m);
            row.set_numeric("fmp_buy_shares_6m", agg.buy_shares_6m);
            row.set_numeric("fmp_sell_shares_6m", agg.sell_shares_6m);
            row.set_numeric("fmp_buy_trans_6m", agg.buy_trans_6m as f64);
            row.set_numeric("fmp_sell_trans_6m", agg.sell_trans_6m as f64);
            Some(row)
        })
        .collect::<Vec<_>>();
    out.sort_by(|a, b| {
        b.numeric("fmp_net_shares_6m")
            .unwrap_or(f64::NEG_INFINITY)
            .partial_cmp(&a.numeric("fmp_net_shares_6m").unwrap_or(f64::NEG_INFINITY))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out.truncate(request.limit);
    Ok(out)
}

fn fetch_fmp_aggregate(
    client: &Client,
    symbol: &str,
    api_key: &str,
    cutoff: NaiveDate,
) -> anyhow::Result<Option<FmpAggregate>> {
    let mut transactions = Vec::new();
    for page in 0..FMP_MAX_PAGES {
        let url = format!(
            "https://financialmodelingprep.com/api/v4/insider-trading?symbol={}&page={page}&apikey={}",
            urlencoding::encode(symbol),
            urlencoding::encode(api_key)
        );
        let rows: Value = client
            .get(url)
            .send()
            .with_context(|| format!("FMP insider request failed for {symbol}"))?
            .error_for_status()
            .with_context(|| format!("FMP insider request returned non-success for {symbol}"))?
            .json()
            .context("FMP insider JSON parse failed")?;
        let Some(page_rows) = rows.as_array() else {
            break;
        };
        if page_rows.is_empty() {
            break;
        }
        let oldest = page_rows.last().and_then(transaction_date);
        transactions.extend(page_rows.iter().cloned());
        if oldest.is_some_and(|day| day < cutoff) {
            break;
        }
    }
    Ok(aggregate_fmp_transactions(&transactions, cutoff))
}

fn aggregate_fmp_transactions(transactions: &[Value], cutoff: NaiveDate) -> Option<FmpAggregate> {
    let mut agg = FmpAggregate::default();
    for txn in transactions {
        let Some(day) = transaction_date(txn) else {
            continue;
        };
        if day < cutoff {
            continue;
        }
        let shares = txn
            .get("securitiesTransacted")
            .and_then(value_as_f64)
            .unwrap_or(0.0);
        let disposition = txn.get("acquistionOrDisposition").and_then(Value::as_str);
        let txn_type = txn
            .get("transactionType")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_uppercase();
        if txn_type.starts_with("P-") && disposition == Some("A") {
            agg.buy_shares_6m += shares;
            agg.buy_trans_6m += 1;
        } else if txn_type.starts_with("S-") && disposition == Some("D") {
            agg.sell_shares_6m += shares;
            agg.sell_trans_6m += 1;
        }
    }
    if agg.buy_trans_6m == 0 && agg.sell_trans_6m == 0 {
        None
    } else {
        agg.net_shares_6m = agg.buy_shares_6m - agg.sell_shares_6m;
        Some(agg)
    }
}

fn transaction_date(txn: &Value) -> Option<NaiveDate> {
    let raw = txn
        .get("transactionDate")
        .or_else(|| txn.get("filingDate"))
        .and_then(Value::as_str)?;
    NaiveDate::parse_from_str(raw.get(..10)?, "%Y-%m-%d").ok()
}

fn value_as_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|s| s.replace(',', "").parse().ok()))
}

fn bare_symbol(raw: &str) -> String {
    raw.split_once(':')
        .map(|(_, rest)| rest)
        .unwrap_or(raw)
        .trim()
        .to_uppercase()
}

fn india_symbol(raw: &str) -> String {
    bare_symbol(raw)
        .trim_end_matches(".NS")
        .trim_end_matches(".BO")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_only_open_market_fmp_purchases_and_sales() {
        let cutoff = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let rows = vec![
            serde_json::json!({"transactionDate": "2026-02-01", "securitiesTransacted": 100, "transactionType": "P-Purchase", "acquistionOrDisposition": "A"}),
            serde_json::json!({"transactionDate": "2026-02-02", "securitiesTransacted": 40, "transactionType": "S-Sale", "acquistionOrDisposition": "D"}),
            serde_json::json!({"transactionDate": "2026-02-03", "securitiesTransacted": 999, "transactionType": "A-Award", "acquistionOrDisposition": "A"}),
        ];
        let agg = aggregate_fmp_transactions(&rows, cutoff).unwrap();
        assert_eq!(agg.buy_shares_6m, 100.0);
        assert_eq!(agg.sell_shares_6m, 40.0);
        assert_eq!(agg.net_shares_6m, 60.0);
    }
}
