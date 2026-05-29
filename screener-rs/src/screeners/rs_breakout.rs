use crate::data::{Bars, PricePanel};
use crate::indicators::atr;
use crate::providers::nse::DeliveryRow;
use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;
use std::collections::BTreeMap;

pub const DEFAULT_RS_WINDOW: usize = 55;
pub const DEFAULT_SUPERTREND_PERIOD: usize = 10;
pub const DEFAULT_SUPERTREND_MULTIPLIER: f64 = 3.0;
pub const DEFAULT_VOLUME_WINDOW: usize = 20;
pub const DEFAULT_VOLUME_MULTIPLIER: f64 = 1.5;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RsBreakoutRow {
    pub symbol: String,
    pub date: NaiveDate,
    pub close: f64,
    pub rs_55: f64,
    pub supertrend: f64,
    pub previous_week_high: Option<f64>,
    pub volume: f64,
    pub avg_volume_20d: f64,
    pub volume_ratio: f64,
    pub delivery_pct: Option<f64>,
    pub previous_delivery_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RsBreakoutResult {
    pub as_of: NaiveDate,
    pub benchmark: String,
    pub full: Vec<RsBreakoutRow>,
    pub relaxed: Vec<RsBreakoutRow>,
}

pub fn scan_rs_breakouts(
    bars_by_symbol: &PricePanel,
    benchmark_bars: &Bars,
    as_of: NaiveDate,
    delivery_panel: &[DeliveryRow],
    benchmark_symbol: &str,
    require_delivery: bool,
) -> anyhow::Result<RsBreakoutResult> {
    if benchmark_bars.is_empty() {
        anyhow::bail!("Benchmark OHLCV data is empty.");
    }
    let delivery = delivery_lookup(delivery_panel);
    let mut full = Vec::new();
    let mut relaxed = Vec::new();
    for (symbol, bars) in bars_by_symbol {
        let bare = india_symbol(symbol);
        let Some((row, price_pass, delivery_pass)) = evaluate_symbol(
            &bare,
            bars,
            benchmark_bars,
            as_of,
            delivery.get(&bare).copied(),
        ) else {
            continue;
        };
        relaxed.push(row.clone());
        if price_pass && (delivery_pass || !require_delivery) {
            full.push(row);
        }
    }
    sort_rows(&mut full);
    sort_rows(&mut relaxed);
    Ok(RsBreakoutResult {
        as_of,
        benchmark: benchmark_symbol.to_string(),
        full,
        relaxed,
    })
}

pub fn evaluate_symbol(
    symbol: &str,
    bars: &Bars,
    benchmark_bars: &Bars,
    as_of: NaiveDate,
    delivery: Option<(Option<f64>, Option<f64>)>,
) -> Option<(RsBreakoutRow, bool, bool)> {
    let bars = bars.between(NaiveDate::MIN, as_of);
    if bars.len()
        < DEFAULT_RS_WINDOW
            .max(DEFAULT_VOLUME_WINDOW)
            .max(DEFAULT_SUPERTREND_PERIOD)
            + 1
    {
        return None;
    }
    let last = bars.rows.last()?;
    let rs_55 = relative_strength_55(&bars, benchmark_bars, last.date)?;
    let st = supertrend_values(
        &bars,
        DEFAULT_SUPERTREND_PERIOD,
        DEFAULT_SUPERTREND_MULTIPLIER,
    );
    let supertrend = *st.last()?;
    if !supertrend.is_finite() {
        return None;
    }
    let avg20 = prior_average_volume(&bars, DEFAULT_VOLUME_WINDOW)?;
    if !avg20.is_finite() || avg20 <= 0.0 {
        return None;
    }
    let volume_ratio = last.volume / avg20;
    let base_pass =
        rs_55 > 0.0 && last.close > supertrend && volume_ratio >= DEFAULT_VOLUME_MULTIPLIER;
    if !base_pass {
        return None;
    }
    let prev_week_high = previous_completed_week_high(&bars, last.date);
    let price_pass = prev_week_high.is_some_and(|high| last.close > high);
    let (delivery_pct, previous_delivery_pct) = delivery.unwrap_or((None, None));
    let delivery_pass = match (delivery_pct, previous_delivery_pct) {
        (Some(latest), Some(prev)) => latest > prev,
        _ => false,
    };
    Some((
        RsBreakoutRow {
            symbol: symbol.to_string(),
            date: last.date,
            close: last.close,
            rs_55: round4(rs_55),
            supertrend: round4(supertrend),
            previous_week_high: prev_week_high.map(round4),
            volume: last.volume,
            avg_volume_20d: round4(avg20),
            volume_ratio: round4(volume_ratio),
            delivery_pct: delivery_pct.map(round4),
            previous_delivery_pct: previous_delivery_pct.map(round4),
        },
        price_pass,
        delivery_pass,
    ))
}

pub fn relative_strength_55(
    stock_bars: &Bars,
    benchmark_bars: &Bars,
    last_date: NaiveDate,
) -> Option<f64> {
    let bench = benchmark_bars
        .rows
        .iter()
        .map(|bar| (bar.date, bar.close))
        .collect::<BTreeMap<_, _>>();
    let aligned = stock_bars
        .rows
        .iter()
        .filter_map(|bar| {
            bench
                .get(&bar.date)
                .map(|bench_close| (bar.date, bar.close, *bench_close))
        })
        .collect::<Vec<_>>();
    let idx = aligned.iter().position(|(date, _, _)| *date == last_date)?;
    if idx < DEFAULT_RS_WINDOW {
        return None;
    }
    let (_, stock_now, bench_now) = aligned[idx];
    let (_, stock_prev, bench_prev) = aligned[idx - DEFAULT_RS_WINDOW];
    if stock_prev <= 0.0 || bench_prev <= 0.0 || bench_now <= 0.0 {
        return None;
    }
    Some(((stock_now / stock_prev) / (bench_now / bench_prev) - 1.0) * 100.0)
}

pub fn supertrend_values(bars: &Bars, period: usize, multiplier: f64) -> Vec<f64> {
    let atr_values = atr(bars, period);
    let mut final_upper = vec![f64::NAN; bars.len()];
    let mut final_lower = vec![f64::NAN; bars.len()];
    let mut st = vec![f64::NAN; bars.len()];
    for i in 0..bars.len() {
        let bar = &bars.rows[i];
        if atr_values[i].is_nan() {
            continue;
        }
        let hl2 = (bar.high + bar.low) / 2.0;
        let basic_upper = hl2 + multiplier * atr_values[i];
        let basic_lower = hl2 - multiplier * atr_values[i];
        if i == 0 || final_upper[i - 1].is_nan() {
            final_upper[i] = basic_upper;
            final_lower[i] = basic_lower;
            st[i] = if bar.close >= hl2 {
                final_lower[i]
            } else {
                final_upper[i]
            };
            continue;
        }
        final_upper[i] =
            if basic_upper < final_upper[i - 1] || bars.rows[i - 1].close > final_upper[i - 1] {
                basic_upper
            } else {
                final_upper[i - 1]
            };
        final_lower[i] =
            if basic_lower > final_lower[i - 1] || bars.rows[i - 1].close < final_lower[i - 1] {
                basic_lower
            } else {
                final_lower[i - 1]
            };
        st[i] = if st[i - 1] == final_upper[i - 1] {
            if bar.close > final_upper[i] {
                final_lower[i]
            } else {
                final_upper[i]
            }
        } else if bar.close < final_lower[i] {
            final_upper[i]
        } else {
            final_lower[i]
        };
    }
    st
}

fn prior_average_volume(bars: &Bars, window: usize) -> Option<f64> {
    if bars.len() <= window {
        return None;
    }
    let end = bars.len() - 1;
    let start = end - window;
    Some(
        bars.rows[start..end]
            .iter()
            .map(|bar| bar.volume)
            .sum::<f64>()
            / window as f64,
    )
}

fn previous_completed_week_high(bars: &Bars, as_of: NaiveDate) -> Option<f64> {
    let this_monday = as_of - Duration::days(as_of.weekday().num_days_from_monday() as i64);
    let prev_monday = this_monday - Duration::days(7);
    let prev_friday = this_monday - Duration::days(3);
    bars.rows
        .iter()
        .filter(|bar| bar.date >= prev_monday && bar.date <= prev_friday)
        .map(|bar| bar.high)
        .reduce(f64::max)
}

fn delivery_lookup(panel: &[DeliveryRow]) -> BTreeMap<String, (Option<f64>, Option<f64>)> {
    let mut grouped: BTreeMap<String, Vec<&DeliveryRow>> = BTreeMap::new();
    for row in panel {
        grouped
            .entry(row.symbol.to_uppercase())
            .or_default()
            .push(row);
    }
    grouped
        .into_iter()
        .filter_map(|(symbol, mut rows)| {
            rows.sort_by_key(|row| row.date);
            let latest = rows.last().map(|row| row.deliv_per);
            let prev = rows
                .get(rows.len().saturating_sub(2))
                .map(|row| row.deliv_per);
            latest.map(|latest| (symbol, (Some(latest), prev)))
        })
        .collect()
}

pub fn india_symbol(symbol: &str) -> String {
    symbol
        .split_once(':')
        .map(|(_, rest)| rest)
        .unwrap_or(symbol)
        .trim_end_matches(".NS")
        .trim_end_matches(".BO")
        .to_uppercase()
}

fn sort_rows(rows: &mut [RsBreakoutRow]) {
    rows.sort_by(|a, b| {
        b.volume_ratio
            .partial_cmp(&a.volume_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                b.rs_55
                    .partial_cmp(&a.rs_55)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Bar;

    fn bars(close_base: f64, volume_last: f64) -> Bars {
        let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        Bars::new(
            (0..70)
                .map(|i| {
                    let day = start + Duration::days(i);
                    let close = close_base + i as f64;
                    Bar {
                        date: day,
                        open: close - 0.5,
                        high: close + if (63..=67).contains(&i) { 0.0 } else { 0.5 },
                        low: close - 1.0,
                        close,
                        volume: if i == 69 { volume_last } else { 100.0 },
                        adj_close: None,
                        dividend: None,
                        extra: BTreeMap::new(),
                    }
                })
                .collect(),
        )
    }

    fn flat_bars(close: f64) -> Bars {
        let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        Bars::new(
            (0..70)
                .map(|i| Bar {
                    date: start + Duration::days(i),
                    open: close,
                    high: close + 0.5,
                    low: close - 0.5,
                    close,
                    volume: 100.0,
                    adj_close: None,
                    dividend: None,
                    extra: BTreeMap::new(),
                })
                .collect(),
        )
    }

    #[test]
    fn evaluates_base_rs_breakout() {
        let stock = bars(100.0, 300.0);
        let bench = flat_bars(100.0);
        let out = evaluate_symbol("ABC", &stock, &bench, stock.rows.last().unwrap().date, None);
        assert!(out.is_some());
    }
}
