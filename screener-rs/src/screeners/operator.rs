use crate::data::tv_to_yf;
use crate::providers::nse::{CashBhavcopyRow, NearMonthOi, NseClient, near_month_oi};
use crate::providers::tradingview::TradingViewClient;
use crate::providers::yahoo::YahooPriceFetcher;
use chrono::{Duration, NaiveDate};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OperatorRow {
    pub symbol: String,
    pub operator_action: Option<String>,
    pub high_momentum_watch: bool,
    pub close: Option<f64>,
    pub vwap: Option<f64>,
    pub pct_change_price: Option<f64>,
    pub pct_change_oi: Option<f64>,
    pub pct_change_delivery: Option<f64>,
    pub dist_from_52w_high: Option<f64>,
    pub high_52w: Option<f64>,
    pub low_52w: Option<f64>,
    pub deliv_qty: Option<f64>,
    pub deliv_pct: Option<f64>,
    pub avg_delivery_5d: Option<f64>,
    pub current_oi: Option<f64>,
    pub next_oi: Option<f64>,
    pub cumulative_oi: Option<f64>,
    pub prev_close: Option<f64>,
    pub is_fno: bool,
}

pub fn build_dataset(
    as_of: NaiveDate,
    universe_mode: &str,
) -> anyhow::Result<(Vec<OperatorRow>, NaiveDate)> {
    let nse = NseClient::new()?;
    let tv = TradingViewClient::new()?;
    let yahoo = YahooPriceFetcher::new()?;
    let actual = nse.latest_trading_day(as_of, 7)?;
    let fo_today = nse.fetch_fo_bhavcopy(actual)?;
    let fno = fo_today
        .iter()
        .map(|row| row.symbol.clone())
        .collect::<BTreeSet<_>>();
    let universe = match universe_mode {
        "fo" => fno.iter().cloned().collect::<Vec<_>>(),
        "fo+cash" => {
            let mut out = fno.iter().cloned().collect::<BTreeSet<_>>();
            if let Ok(cash) = tv.liquid_universe("india", 500, 50.0, None) {
                for row in cash {
                    if let Some(name) = row.name {
                        out.insert(name);
                    }
                }
            }
            out.into_iter().collect()
        }
        other => anyhow::bail!("unknown operator universe mode {other:?}"),
    };

    let cash_today = nse.fetch_cash_bhavcopy(actual)?;
    let avg_delivery = five_day_avg_delivery(&nse, actual)?;
    let oi_today = near_month_oi(&fo_today);
    let prev_day = nse.latest_trading_day(actual - Duration::days(1), 7)?;
    let oi_prev = near_month_oi(&nse.fetch_fo_bhavcopy(prev_day)?);
    let hl = fifty_two_week_hl(&yahoo, &universe, actual);

    let cash_map = cash_today
        .into_iter()
        .map(|row| (row.symbol.clone(), row))
        .collect::<BTreeMap<_, _>>();
    let avg_map = avg_delivery;
    let oi_map = oi_today
        .into_iter()
        .map(|row| (row.symbol.clone(), row))
        .collect::<BTreeMap<_, _>>();
    let prev_oi_map = oi_prev
        .into_iter()
        .map(|row| (row.symbol.clone(), row.cumulative_oi))
        .collect::<BTreeMap<_, _>>();

    let mut rows = universe
        .into_iter()
        .map(|symbol| {
            make_row(
                &symbol,
                cash_map.get(&symbol),
                avg_map.get(&symbol).copied(),
                oi_map.get(&symbol),
                prev_oi_map.get(&symbol).copied().flatten(),
                hl.get(&symbol).copied(),
                fno.contains(&symbol),
            )
        })
        .collect::<Vec<_>>();
    rows.sort_by(sort_operator_rows);
    Ok((rows, actual))
}

pub fn write_csv(
    rows: &[OperatorRow],
    as_of: NaiveDate,
    out_path: Option<&Path>,
    only_actions: bool,
) -> anyhow::Result<PathBuf> {
    let path = out_path.map(Path::to_path_buf).unwrap_or_else(|| {
        PathBuf::from(format!(
            "daily_operator_data_{}.csv",
            as_of.format("%Y%m%d")
        ))
    });
    let mut writer = csv::Writer::from_path(&path)?;
    writer.write_record([
        "SYMBOL",
        "Operator_Action",
        "High_Momentum_Watch",
        "Close",
        "VWAP",
        "%_Change_Price",
        "%_Change_OI",
        "%_Change_Delivery",
        "Dist_From_52W_High",
        "52W_High",
        "52W_Low",
        "Deliv_Qty",
        "Deliv_Pct",
        "5_Day_Avg_Delivery",
        "Current_OI",
        "Next_OI",
        "Cumulative_OI",
        "Prev_Close",
    ])?;
    for row in rows
        .iter()
        .filter(|row| !only_actions || row.operator_action.is_some())
    {
        writer.write_record([
            row.symbol.clone(),
            row.operator_action.clone().unwrap_or_default(),
            row.high_momentum_watch.to_string(),
            fmt(row.close),
            fmt(row.vwap),
            fmt(row.pct_change_price),
            fmt(row.pct_change_oi),
            fmt(row.pct_change_delivery),
            fmt(row.dist_from_52w_high),
            fmt(row.high_52w),
            fmt(row.low_52w),
            fmt(row.deliv_qty),
            fmt(row.deliv_pct),
            fmt(row.avg_delivery_5d),
            fmt(row.current_oi),
            fmt(row.next_oi),
            fmt(row.cumulative_oi),
            fmt(row.prev_close),
        ])?;
    }
    writer.flush()?;
    Ok(path)
}

fn make_row(
    symbol: &str,
    cash: Option<&CashBhavcopyRow>,
    avg_delivery_5d: Option<f64>,
    oi: Option<&NearMonthOi>,
    prev_cumulative_oi: Option<f64>,
    hl: Option<(Option<f64>, Option<f64>)>,
    is_fno: bool,
) -> OperatorRow {
    let close = cash.map(|row| row.close_price).filter(|v| v.is_finite());
    let prev_close = cash.map(|row| row.prev_close).filter(|v| v.is_finite());
    let cumulative_oi = oi.and_then(|row| row.cumulative_oi);
    let pct_change_price = match (close, prev_close) {
        (Some(close), Some(prev)) if prev != 0.0 => Some((close / prev - 1.0) * 100.0),
        _ => None,
    };
    let pct_change_oi = match (cumulative_oi, prev_cumulative_oi) {
        (Some(cur), Some(prev)) if prev != 0.0 => Some((cur / prev - 1.0) * 100.0),
        _ => None,
    };
    let pct_change_delivery = match (cash.map(|row| row.deliv_qty), avg_delivery_5d) {
        (Some(deliv), Some(avg)) if avg != 0.0 => Some((deliv / avg) * 100.0),
        _ => None,
    };
    let (high_52w, low_52w) = hl.unwrap_or((None, None));
    let dist_from_52w_high = match (high_52w, close) {
        (Some(high), Some(close)) if high != 0.0 => Some((high - close) / high * 100.0),
        _ => None,
    };
    let operator_action = classify(is_fno, pct_change_price, pct_change_oi, pct_change_delivery);
    let high_momentum_watch = operator_action.as_deref() == Some("Long Build-up")
        && dist_from_52w_high.is_some_and(|dist| dist <= 15.0);
    OperatorRow {
        symbol: symbol.to_string(),
        operator_action,
        high_momentum_watch,
        close,
        vwap: cash.map(|row| row.avg_price).filter(|v| v.is_finite()),
        pct_change_price: pct_change_price.map(round4),
        pct_change_oi: pct_change_oi.map(round4),
        pct_change_delivery: pct_change_delivery.map(round4),
        dist_from_52w_high: dist_from_52w_high.map(round4),
        high_52w,
        low_52w,
        deliv_qty: cash.map(|row| row.deliv_qty).filter(|v| v.is_finite()),
        deliv_pct: cash.map(|row| row.deliv_per).filter(|v| v.is_finite()),
        avg_delivery_5d,
        current_oi: oi.and_then(|row| row.current_oi),
        next_oi: oi.and_then(|row| row.next_oi),
        cumulative_oi,
        prev_close,
        is_fno,
    }
}

fn classify(
    is_fno: bool,
    price: Option<f64>,
    oi: Option<f64>,
    delivery: Option<f64>,
) -> Option<String> {
    if !is_fno || delivery? <= 100.0 {
        return None;
    }
    let p = price?;
    let oi = oi?;
    if p > 0.0 && oi > 0.0 {
        Some("Long Build-up".to_string())
    } else if p > 0.0 && oi < 0.0 {
        Some("Short Covering".to_string())
    } else if p < 0.0 && oi > 0.0 {
        Some("Short Build-up".to_string())
    } else if p < 0.0 && oi < 0.0 {
        Some("Long Unwinding".to_string())
    } else {
        None
    }
}

fn five_day_avg_delivery(
    nse: &NseClient,
    as_of: NaiveDate,
) -> anyhow::Result<BTreeMap<String, f64>> {
    let mut values: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut day = as_of - Duration::days(1);
    let mut collected = 0;
    while collected < 5 {
        let td = nse.latest_trading_day(day, 7)?;
        let rows = nse.fetch_cash_bhavcopy(td)?;
        for row in rows {
            values.entry(row.symbol).or_default().push(row.deliv_qty);
        }
        collected += 1;
        day = td - Duration::days(1);
    }
    Ok(values
        .into_iter()
        .filter_map(|(symbol, vals)| {
            (!vals.is_empty()).then_some((symbol, vals.iter().sum::<f64>() / vals.len() as f64))
        })
        .collect())
}

fn fifty_two_week_hl(
    yahoo: &YahooPriceFetcher,
    symbols: &[String],
    as_of: NaiveDate,
) -> BTreeMap<String, (Option<f64>, Option<f64>)> {
    let mut out = BTreeMap::new();
    let start = as_of - Duration::days(400);
    for symbol in symbols {
        let yf = tv_to_yf(symbol, "india");
        let value = yahoo.fetch_symbol(&yf, start, as_of).ok().and_then(|bars| {
            (bars.len() >= 200).then(|| {
                let high = bars.rows.iter().map(|bar| bar.high).reduce(f64::max);
                let low = bars.rows.iter().map(|bar| bar.low).reduce(f64::min);
                (high, low)
            })
        });
        out.insert(symbol.clone(), value.unwrap_or((None, None)));
    }
    out
}

fn sort_operator_rows(a: &OperatorRow, b: &OperatorRow) -> std::cmp::Ordering {
    let a_hmw = if a.high_momentum_watch { 0 } else { 1 };
    let b_hmw = if b.high_momentum_watch { 0 } else { 1 };
    a_hmw
        .cmp(&b_hmw)
        .then_with(|| {
            action_rank(a.operator_action.as_deref())
                .cmp(&action_rank(b.operator_action.as_deref()))
        })
        .then_with(|| {
            b.pct_change_delivery
                .unwrap_or(f64::NEG_INFINITY)
                .partial_cmp(&a.pct_change_delivery.unwrap_or(f64::NEG_INFINITY))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

fn action_rank(action: Option<&str>) -> usize {
    match action {
        Some("Long Build-up") => 0,
        Some("Short Covering") => 1,
        Some("Short Build-up") => 2,
        Some("Long Unwinding") => 3,
        _ => 99,
    }
}

fn fmt(value: Option<f64>) -> String {
    match value {
        Some(v) if v.is_finite() => format!("{v:.4}"),
        _ => String::new(),
    }
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_operator_actions_with_delivery_gate() {
        assert_eq!(
            classify(true, Some(1.0), Some(1.0), Some(101.0)).as_deref(),
            Some("Long Build-up")
        );
        assert_eq!(classify(true, Some(1.0), Some(1.0), Some(100.0)), None);
        assert_eq!(classify(false, Some(1.0), Some(1.0), Some(101.0)), None);
    }
}
