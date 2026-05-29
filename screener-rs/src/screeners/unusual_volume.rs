use crate::data::{Bars, PricePanel};
use crate::providers::nse::DeliveryRow;
use chrono::NaiveDate;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub const DEFAULT_MIN_RVOL: f64 = 2.0;
pub const DEFAULT_MIN_Z: f64 = 2.0;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnusualVolumeEvent {
    pub symbol: String,
    pub date: NaiveDate,
    pub close: f64,
    pub pct_change: f64,
    pub volume: f64,
    pub avg_volume_20d: f64,
    pub rvol: f64,
    pub rvol_5d: f64,
    pub rvol_50d: f64,
    pub rvol_90d: f64,
    pub z_score: f64,
    pub pct_rank_252d: f64,
    pub direction: String,
    pub strength: String,
    pub delivery_qty: Option<f64>,
    pub delivery_pct: Option<f64>,
    pub delivery_rvol: Option<f64>,
    pub conviction_score: Option<f64>,
    pub market_cap: Option<f64>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnusualVolumeResult {
    pub events: Vec<UnusualVolumeEvent>,
    pub fetched_count: usize,
    pub liquid_count: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct UnusualVolumeScanRequest<'a> {
    pub bars_by_symbol: &'a PricePanel,
    pub as_of: NaiveDate,
    pub min_rvol: f64,
    pub min_z: f64,
    pub strength_floor: &'a str,
    pub min_avg_volume: f64,
    pub min_market_cap: f64,
    pub market_caps: &'a BTreeMap<String, f64>,
    pub delivery_panel: &'a [DeliveryRow],
    pub banned_symbols: &'a BTreeSet<String>,
}

pub fn run_unusual_volume_scan(request: UnusualVolumeScanRequest<'_>) -> UnusualVolumeResult {
    let liquid = request
        .bars_by_symbol
        .iter()
        .filter(|(symbol, bars)| {
            !request.banned_symbols.contains(&india_symbol(symbol))
                && passes_volume_floor(bars, request.min_avg_volume, request.as_of)
        })
        .collect::<Vec<_>>();
    let mut events = liquid
        .iter()
        .filter_map(|(symbol, bars)| {
            detect_ticker(symbol, bars, request.as_of, request.min_rvol, request.min_z)
        })
        .collect::<Vec<_>>();
    overlay_delivery(&mut events, request.delivery_panel);
    for event in &mut events {
        event.market_cap = request.market_caps.get(&event.symbol).copied();
    }
    let floor = strength_rank(request.strength_floor);
    events.retain(|event| {
        strength_rank(&event.strength) >= floor
            && event
                .market_cap
                .map(|cap| cap >= request.min_market_cap)
                .unwrap_or(true)
    });
    sort_events(&mut events);
    UnusualVolumeResult {
        events,
        fetched_count: request.bars_by_symbol.len(),
        liquid_count: liquid.len(),
    }
}

pub fn detect_ticker(
    symbol: &str,
    bars: &Bars,
    as_of: NaiveDate,
    min_rvol: f64,
    min_z: f64,
) -> Option<UnusualVolumeEvent> {
    let bars = bars.between(NaiveDate::MIN, as_of);
    if bars.is_empty() {
        return None;
    }
    let last = bars.rows.last()?;
    if last.date != as_of && (as_of - last.date).num_days() > 7 {
        return None;
    }
    let volumes = bars.rows.iter().map(|bar| bar.volume).collect::<Vec<_>>();
    let idx = volumes.len() - 1;
    let avg20 = prior_mean(&volumes, idx, 20)?;
    if avg20 <= 0.0 || !avg20.is_finite() {
        return None;
    }
    let rvol_5 = ratio(last.volume, prior_mean(&volumes, idx, 5));
    let rvol_20 = ratio(last.volume, Some(avg20));
    let rvol_50 = ratio(last.volume, prior_mean(&volumes, idx, 50));
    let rvol_90 = ratio(last.volume, prior_mean(&volumes, idx, 90));
    let z = prior_z_score(&volumes, idx, 90);
    let emit_rvol = max_finite(rvol_20, rvol_5);
    let emit_z = finite_or_zero(z);
    if emit_rvol < min_rvol && emit_z < min_z {
        return None;
    }
    let prev_close = if bars.len() >= 2 {
        bars.rows[bars.len() - 2].close
    } else {
        last.close
    };
    let pct_change = if prev_close > 0.0 {
        (last.close - prev_close) / prev_close * 100.0
    } else {
        0.0
    };
    let direction = classify_direction(last.open, last.high, last.low, last.close, prev_close);
    let strength = classify_strength(finite_or_zero(rvol_20), finite_or_zero(z));
    Some(UnusualVolumeEvent {
        symbol: india_symbol(symbol),
        date: last.date,
        close: last.close,
        pct_change: round4(pct_change),
        volume: last.volume,
        avg_volume_20d: avg20,
        rvol: round4_or_nan(rvol_20),
        rvol_5d: round4_or_nan(rvol_5),
        rvol_50d: round4_or_nan(rvol_50),
        rvol_90d: round4_or_nan(rvol_90),
        z_score: round4_or_nan(z),
        pct_rank_252d: round4_or_nan(pct_rank_last(&volumes, idx, 252)),
        direction,
        strength,
        delivery_qty: None,
        delivery_pct: None,
        delivery_rvol: None,
        conviction_score: None,
        market_cap: None,
        notes: String::new(),
    })
}

pub fn passes_volume_floor(bars: &Bars, min_avg_volume: f64, as_of: NaiveDate) -> bool {
    let bars = bars.between(NaiveDate::MIN, as_of);
    if bars.len() < 21 {
        return false;
    }
    let volumes = bars.rows.iter().map(|bar| bar.volume).collect::<Vec<_>>();
    prior_mean(&volumes, volumes.len() - 1, 20).is_some_and(|avg| avg >= min_avg_volume)
}

fn classify_direction(open_px: f64, high: f64, low: f64, close: f64, prev_close: f64) -> String {
    let range = (high - low).max(1e-9);
    if prev_close > 0.0 {
        let gap = (open_px - prev_close) / prev_close;
        let change = (close - prev_close) / prev_close;
        if gap.abs() > 0.02 && gap * change < 0.0 {
            return "REVERSAL".to_string();
        }
        if change.abs() < 0.01 {
            return "CHURN".to_string();
        }
    }
    let upper_third = low + range * (2.0 / 3.0);
    let lower_third = low + range * (1.0 / 3.0);
    if close > open_px && close >= upper_third {
        "BUYING".to_string()
    } else if close < open_px && close <= lower_third {
        "SELLING".to_string()
    } else {
        "CHURN".to_string()
    }
}

fn classify_strength(rvol: f64, z: f64) -> String {
    if rvol >= 5.0 || z >= 3.5 {
        "EXTREME".to_string()
    } else if rvol >= 3.0 || z >= 2.5 {
        "HIGH".to_string()
    } else {
        "MODERATE".to_string()
    }
}

fn overlay_delivery(events: &mut [UnusualVolumeEvent], panel: &[DeliveryRow]) {
    if events.is_empty() || panel.is_empty() {
        return;
    }
    let mut grouped: BTreeMap<String, Vec<&DeliveryRow>> = BTreeMap::new();
    for row in panel {
        grouped
            .entry(row.symbol.to_uppercase())
            .or_default()
            .push(row);
    }
    let mut metrics = BTreeMap::new();
    for (symbol, mut rows) in grouped {
        rows.sort_by_key(|row| row.date);
        for i in 0..rows.len() {
            let prior = if i >= 5 {
                let start = i.saturating_sub(20);
                Some(
                    rows[start..i].iter().map(|row| row.deliv_qty).sum::<f64>()
                        / (i - start) as f64,
                )
            } else {
                None
            };
            let trend_prior = if i + 1 >= 5 {
                let start = (i + 1).saturating_sub(20);
                let window = &rows[start..=i];
                Some(window.iter().map(|row| row.deliv_per).sum::<f64>() / window.len() as f64)
            } else {
                None
            };
            let spike_std = trend_prior.and_then(|mean| {
                let start = (i + 1).saturating_sub(20);
                let window = &rows[start..=i];
                let var = window
                    .iter()
                    .map(|row| (row.deliv_per - mean).powi(2))
                    .sum::<f64>()
                    / window.len() as f64;
                (var > 0.0).then_some(var.sqrt())
            });
            let delivery_rvol =
                prior.and_then(|avg| (avg > 0.0).then_some(rows[i].deliv_qty / avg));
            let delivery_trend =
                trend_prior.and_then(|avg| (avg > 0.0).then_some(rows[i].deliv_per / avg));
            let delivery_spike = match (trend_prior, spike_std) {
                (Some(mean), Some(sd)) => Some((rows[i].deliv_per - mean) / sd),
                _ => None,
            };
            metrics.insert(
                (symbol.clone(), rows[i].date),
                (
                    rows[i].deliv_qty,
                    rows[i].deliv_per,
                    delivery_rvol,
                    delivery_trend,
                    delivery_spike,
                ),
            );
        }
    }
    for event in events {
        let Some((qty, pct, rvol, _trend, _spike)) = metrics
            .get(&(event.symbol.to_uppercase(), event.date))
            .copied()
        else {
            continue;
        };
        event.delivery_qty = Some(qty);
        event.delivery_pct = Some(pct);
        event.delivery_rvol = rvol;
        if event.rvol.is_finite() {
            event.conviction_score = Some(round4(event.rvol * pct / 100.0));
        }
        let notes = delivery_notes(event.delivery_rvol.unwrap_or(0.0), pct, &event.direction);
        if !notes.is_empty() {
            event.notes = notes;
        }
    }
}

fn delivery_notes(rvol: f64, delivery_pct: f64, direction: &str) -> String {
    let mut notes = Vec::new();
    if rvol >= 3.0 && delivery_pct >= 50.0 {
        notes.push("strong institutional footprint");
    } else if rvol >= 3.0 && delivery_pct < 25.0 {
        notes.push("speculative/operator-driven; low conviction");
    }
    if direction == "SELLING" && rvol >= 3.0 && delivery_pct > 60.0 {
        notes.push("long-holder distribution");
    }
    notes.join("; ")
}

pub fn sort_events(events: &mut [UnusualVolumeEvent]) {
    events.sort_by(|a, b| {
        strength_rank(&b.strength)
            .cmp(&strength_rank(&a.strength))
            .then_with(|| {
                b.rvol
                    .partial_cmp(&a.rvol)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
}

fn strength_rank(raw: &str) -> usize {
    match raw.to_ascii_uppercase().as_str() {
        "EXTREME" => 3,
        "HIGH" => 2,
        _ => 1,
    }
}

fn prior_mean(values: &[f64], idx: usize, window: usize) -> Option<f64> {
    if idx < window {
        return None;
    }
    let slice = &values[idx - window..idx];
    if slice.iter().all(|v| v.is_finite()) {
        Some(slice.iter().sum::<f64>() / window as f64)
    } else {
        None
    }
}

fn prior_z_score(values: &[f64], idx: usize, window: usize) -> f64 {
    let Some(mean) = prior_mean(values, idx, window) else {
        return f64::NAN;
    };
    let slice = &values[idx - window..idx];
    let var = slice.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / window as f64;
    if var <= 0.0 {
        f64::NAN
    } else {
        (values[idx] - mean) / var.sqrt()
    }
}

fn pct_rank_last(values: &[f64], idx: usize, window: usize) -> f64 {
    if idx + 1 < window {
        return f64::NAN;
    }
    let slice = &values[idx + 1 - window..=idx];
    slice.iter().filter(|v| **v <= values[idx]).count() as f64 / slice.len() as f64
}

fn ratio(num: f64, denom: Option<f64>) -> f64 {
    match denom {
        Some(denom) if denom > 0.0 => num / denom,
        _ => f64::NAN,
    }
}

fn max_finite(a: f64, b: f64) -> f64 {
    finite_or_zero(a).max(finite_or_zero(b))
}

fn finite_or_zero(value: f64) -> f64 {
    if value.is_finite() { value } else { 0.0 }
}

fn round4_or_nan(value: f64) -> f64 {
    if value.is_finite() {
        round4(value)
    } else {
        f64::NAN
    }
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

fn india_symbol(symbol: &str) -> String {
    symbol
        .split_once(':')
        .map(|(_, rest)| rest)
        .unwrap_or(symbol)
        .trim_end_matches(".NS")
        .trim_end_matches(".BO")
        .to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Bar;
    use chrono::Duration;

    #[test]
    fn detects_rvol_event_against_prior_average() {
        let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let bars = Bars::new(
            (0..100)
                .map(|i| {
                    let volume = if i == 99 { 300.0 } else { 100.0 };
                    Bar {
                        date: start + Duration::days(i),
                        open: 10.0,
                        high: 12.0,
                        low: 9.0,
                        close: if i == 99 { 11.8 } else { 10.0 },
                        volume,
                        adj_close: None,
                        dividend: None,
                    }
                })
                .collect(),
        );
        let event = detect_ticker("ABC", &bars, start + Duration::days(99), 2.0, 2.0).unwrap();
        assert_eq!(event.rvol, 3.0);
        assert_eq!(event.strength, "HIGH");
        assert_eq!(event.direction, "BUYING");
    }
}
