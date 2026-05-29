use crate::backtester::metrics::compute_metrics;
use crate::backtester::models::{
    BacktestConfig, BacktestResult, BarsByTicker, EntryOrderType, ExitReason, PriceAdjustment,
    SelectionRow, SimOutcome, Trade,
};
use crate::backtester::portfolio::{Portfolio, build_equity_curve};
use crate::backtester::slippage::{Side, apply_slippage};
use crate::data::{Bars, PricePanel, business_days, tv_to_yf};
use crate::pine::{Node, evaluate, parse, required_lookback};
use chrono::{Duration, NaiveDate};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

pub trait PriceFetcher {
    fn fetch(
        &self,
        tickers: &[String],
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<PricePanel>;
}

#[derive(Debug, Clone)]
struct SlotState {
    ticker: String,
    entry_idx: usize,
    entry_date: NaiveDate,
    entry_fill: f64,
    signal_date: NaiveDate,
    stop_ref: Option<f64>,
    target_ref: Option<f64>,
    hold_limit_idx: usize,
    peak: f64,
    exit_signal: Option<Vec<f64>>,
    adv_shares: f64,
    sigma_daily: f64,
    partial_targets: Vec<f64>,
    partial_fractions: Vec<f64>,
    partial_fired: Vec<bool>,
}

fn slip(
    ref_price: f64,
    side: Side,
    cfg: &BacktestConfig,
    adv_shares: f64,
    sigma_daily: f64,
) -> f64 {
    apply_slippage(
        &cfg.slippage_model,
        ref_price,
        side,
        0.0,
        adv_shares,
        sigma_daily,
    )
}

fn trailing_liquidity(bars: &Bars, signal_idx: usize, window: usize) -> (f64, f64) {
    if bars.is_empty() || window == 0 {
        return (0.0, 0.0);
    }
    let start = signal_idx.saturating_sub(window - 1);
    let rows = &bars.rows[start..=signal_idx.min(bars.len() - 1)];
    let adv = rows.iter().map(|bar| bar.volume).sum::<f64>() / rows.len() as f64;
    if rows.len() < 2 {
        return (finite_or_zero(adv), 0.0);
    }
    let mut returns = Vec::new();
    for pair in rows.windows(2) {
        if pair[0].close != 0.0 {
            returns.push(pair[1].close / pair[0].close - 1.0);
        }
    }
    let sigma = if returns.is_empty() {
        0.0
    } else {
        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        (returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64).sqrt()
    };
    (finite_or_zero(adv), finite_or_zero(sigma))
}

fn finite_or_zero(value: f64) -> f64 {
    if value.is_finite() { value } else { 0.0 }
}

fn max_concurrent_per_ticker(cfg: &BacktestConfig) -> usize {
    cfg.max_concurrent_per_ticker.max(1)
}

fn raise_if_position_exists(cfg: &BacktestConfig) -> bool {
    max_concurrent_per_ticker(cfg) == 1
}

fn active_ticker_counts(
    slot_states: &BTreeMap<usize, Option<SlotState>>,
) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for state in slot_states.values().flatten() {
        *counts.entry(state.ticker.clone()).or_default() += 1;
    }
    counts
}

fn tickers_at_concurrency_cap(
    slot_states: &BTreeMap<usize, Option<SlotState>>,
    cfg: &BacktestConfig,
) -> BTreeSet<String> {
    let cap = max_concurrent_per_ticker(cfg);
    active_ticker_counts(slot_states)
        .into_iter()
        .filter_map(|(ticker, count)| (count >= cap).then_some(ticker))
        .collect()
}

fn ticker_below_concurrency_cap(
    slot_states: &BTreeMap<usize, Option<SlotState>>,
    ticker: &str,
    cfg: &BacktestConfig,
) -> bool {
    active_ticker_counts(slot_states)
        .get(ticker)
        .copied()
        .unwrap_or(0)
        < max_concurrent_per_ticker(cfg)
}

fn resolve_entry_fill(
    bars: &Bars,
    signal_idx: usize,
    cfg: &BacktestConfig,
) -> (Option<usize>, Option<f64>, Option<String>) {
    if signal_idx + 1 >= bars.len() {
        return (None, None, Some("no post-signal entry bar".to_string()));
    }
    match cfg.entry_order_type {
        EntryOrderType::Moo => {
            let idx = signal_idx + 1;
            (Some(idx), Some(bars.rows[idx].open), None)
        }
        EntryOrderType::Moc => {
            let idx = signal_idx + 1;
            (Some(idx), Some(bars.rows[idx].close), None)
        }
        EntryOrderType::Limit => {
            let Some(limit_bps) = cfg.entry_limit_bps else {
                return (
                    None,
                    None,
                    Some("limit order requires entry_limit_bps".to_string()),
                );
            };
            let signal_close = bars.rows[signal_idx].close;
            let limit_price = signal_close * (1.0 - limit_bps / 10_000.0);
            for i in signal_idx + 1..bars.len() {
                let bar = &bars.rows[i];
                if bar.low <= limit_price {
                    return (Some(i), Some(bar.open.min(limit_price)), None);
                }
            }
            (
                None,
                None,
                Some("limit order never filled in available window".to_string()),
            )
        }
    }
}

fn make_slot_state(
    ticker: &str,
    bars: &Bars,
    signal_idx: usize,
    cfg: &BacktestConfig,
    exit_ast: Option<&Node>,
    _rank: usize,
) -> anyhow::Result<(Option<SlotState>, Option<String>)> {
    let (entry_idx, entry_ref, warning) = resolve_entry_fill(bars, signal_idx, cfg);
    let (Some(entry_idx), Some(entry_ref)) = (entry_idx, entry_ref) else {
        return Ok((None, warning));
    };
    let (adv_shares, sigma_daily) = trailing_liquidity(bars, signal_idx, 20);
    let entry_fill = slip(entry_ref, Side::Buy, cfg, adv_shares, sigma_daily);
    let exit_signal = match exit_ast {
        Some(ast) => {
            Some(evaluate(ast, bars).map_err(|err| anyhow::anyhow!("exit eval failed: {err}"))?)
        }
        None => None,
    };
    let stop_ref = cfg.stop_loss.map(|pct| entry_fill * (1.0 - pct));
    let target_ref = cfg.take_profit.map(|pct| entry_fill * (1.0 + pct));
    let partial_targets = cfg
        .partial_exits
        .iter()
        .map(|(pct, _)| entry_fill * (1.0 + pct))
        .collect::<Vec<_>>();
    let partial_fractions = cfg
        .partial_exits
        .iter()
        .map(|(_, fraction)| *fraction)
        .collect::<Vec<_>>();
    Ok((
        Some(SlotState {
            ticker: ticker.to_string(),
            entry_idx,
            entry_date: bars.rows[entry_idx].date,
            entry_fill,
            signal_date: bars.rows[signal_idx].date,
            stop_ref,
            target_ref,
            hold_limit_idx: entry_idx + cfg.hold,
            peak: entry_fill,
            exit_signal,
            adv_shares,
            sigma_daily,
            partial_targets,
            partial_fractions,
            partial_fired: vec![false; cfg.partial_exits.len()],
        }),
        None,
    ))
}

fn resolve_stop_fill(bar_open: f64, stop_ref: f64, gap_fills: bool) -> f64 {
    if gap_fills && bar_open <= stop_ref {
        bar_open
    } else {
        stop_ref
    }
}

fn resolve_target_fill(bar_open: f64, target_ref: f64, gap_fills: bool) -> f64 {
    if gap_fills && bar_open >= target_ref {
        bar_open
    } else {
        target_ref
    }
}

fn maybe_credit_dividends(
    portfolio: &mut Portfolio,
    state: &SlotState,
    bars: &Bars,
    idx: usize,
    cfg: &BacktestConfig,
) {
    if cfg.price_adjustment == PriceAdjustment::Full {
        return;
    }
    let dividend = bars.rows[idx].dividend.unwrap_or(0.0);
    if dividend.is_finite() && dividend > 0.0 {
        portfolio.credit_dividends(&state.ticker, dividend);
    }
}

fn fire_partial_exits_at_bar(
    state: &mut SlotState,
    bars: &Bars,
    idx: usize,
    cfg: &BacktestConfig,
    portfolio: &mut Portfolio,
) -> anyhow::Result<()> {
    if state.partial_targets.is_empty() || portfolio.get_position(&state.ticker).is_none() {
        return Ok(());
    }
    let bar = &bars.rows[idx];
    for tier_idx in 0..state.partial_targets.len() {
        if state.partial_fired[tier_idx] || bar.high < state.partial_targets[tier_idx] {
            continue;
        }
        let reference =
            resolve_target_fill(bar.open, state.partial_targets[tier_idx], cfg.gap_fills);
        let fill = slip(
            reference,
            Side::Sell,
            cfg,
            state.adv_shares,
            state.sigma_daily,
        );
        portfolio.partial_close(
            &state.ticker,
            bar.date,
            fill,
            ExitReason::Target,
            state.partial_fractions[tier_idx],
            cfg.commission_bps,
        )?;
        state.partial_fired[tier_idx] = true;
        if state.stop_ref.is_none_or(|stop| stop < state.entry_fill) {
            state.stop_ref = Some(state.entry_fill);
        }
    }
    Ok(())
}

fn check_exit_at_bar(
    state: &mut SlotState,
    bars: &Bars,
    idx: usize,
    cfg: &BacktestConfig,
) -> Option<(f64, ExitReason)> {
    let bar = &bars.rows[idx];
    let trail_ref = cfg.trailing_stop.map(|trail| state.peak * (1.0 - trail));
    let stop_hit = state.stop_ref.is_some_and(|stop| bar.low <= stop);
    let target_hit = state.target_ref.is_some_and(|target| bar.high >= target);
    let trail_hit = trail_ref.is_some_and(|trail| bar.low <= trail);

    if stop_hit && target_hit {
        let fill = slip(
            resolve_stop_fill(bar.open, state.stop_ref.unwrap(), cfg.gap_fills),
            Side::Sell,
            cfg,
            state.adv_shares,
            state.sigma_daily,
        );
        return Some((fill, ExitReason::Stop));
    }
    if stop_hit {
        let fill = slip(
            resolve_stop_fill(bar.open, state.stop_ref.unwrap(), cfg.gap_fills),
            Side::Sell,
            cfg,
            state.adv_shares,
            state.sigma_daily,
        );
        return Some((fill, ExitReason::Stop));
    }
    if trail_hit {
        let fill = slip(
            resolve_stop_fill(bar.open, trail_ref.unwrap(), cfg.gap_fills),
            Side::Sell,
            cfg,
            state.adv_shares,
            state.sigma_daily,
        );
        return Some((fill, ExitReason::Trail));
    }
    if target_hit {
        let fill = slip(
            resolve_target_fill(bar.open, state.target_ref.unwrap(), cfg.gap_fills),
            Side::Sell,
            cfg,
            state.adv_shares,
            state.sigma_daily,
        );
        return Some((fill, ExitReason::Target));
    }
    if bar.high > state.peak {
        state.peak = bar.high;
    }
    if state
        .exit_signal
        .as_ref()
        .and_then(|series| series.get(idx))
        .is_some_and(|value| *value != 0.0)
    {
        return Some((
            slip(
                bar.close,
                Side::Sell,
                cfg,
                state.adv_shares,
                state.sigma_daily,
            ),
            ExitReason::ExitExpr,
        ));
    }
    if idx >= state.hold_limit_idx {
        return Some((
            slip(
                bar.close,
                Side::Sell,
                cfg,
                state.adv_shares,
                state.sigma_daily,
            ),
            ExitReason::Time,
        ));
    }
    None
}

fn make_exit(
    entry_date: NaiveDate,
    entry_fill: f64,
    exit_date: NaiveDate,
    exit_fill: f64,
    reason: ExitReason,
    signal_date: NaiveDate,
) -> Trade {
    Trade {
        ticker: String::new(),
        rank: 0,
        signal_date,
        entry_date,
        entry_price: entry_fill,
        exit_date,
        exit_price: exit_fill,
        exit_reason: reason,
        shares: 0.0,
        entry_cost: 0.0,
        exit_value: 0.0,
        pnl: 0.0,
        return_pct: 0.0,
        dividend_income: 0.0,
    }
}

pub fn simulate_ticker(
    bars: &Bars,
    signal_idx: usize,
    cfg: &BacktestConfig,
    exit_ast: Option<&Node>,
) -> anyhow::Result<SimOutcome> {
    let (state, warning) = make_slot_state("", bars, signal_idx, cfg, exit_ast, 0)?;
    let Some(mut state) = state else {
        return Ok(SimOutcome {
            trade: None,
            warning,
        });
    };
    for idx in state.entry_idx + 1..bars.len() {
        if let Some((fill, reason)) = check_exit_at_bar(&mut state, bars, idx, cfg) {
            return Ok(SimOutcome {
                trade: Some(make_exit(
                    state.entry_date,
                    state.entry_fill,
                    bars.rows[idx].date,
                    fill,
                    reason,
                    state.signal_date,
                )),
                warning: None,
            });
        }
    }
    let last = bars.rows.last().expect("non-empty bars");
    let fill = slip(
        last.close,
        Side::Sell,
        cfg,
        state.adv_shares,
        state.sigma_daily,
    );
    Ok(SimOutcome {
        trade: Some(make_exit(
            state.entry_date,
            state.entry_fill,
            last.date,
            fill,
            ExitReason::Eod,
            state.signal_date,
        )),
        warning: None,
    })
}

fn passes_entry_filters(
    bars: &Bars,
    as_of: NaiveDate,
    cfg: &BacktestConfig,
) -> (bool, Option<String>) {
    if cfg.min_price.is_none() && cfg.min_avg_dollar_volume.is_none() {
        return (true, None);
    }
    let Some(pos) = bars.position_on_or_before(as_of) else {
        return (false, Some("no history".to_string()));
    };
    let close = bars.rows[pos].close;
    if let Some(min_price) = cfg.min_price
        && close < min_price
    {
        return (false, Some(format!("price {close:.4} < {min_price}")));
    }
    if let Some(min_adv) = cfg.min_avg_dollar_volume {
        let window = cfg.avg_dollar_volume_window.max(1);
        let start = (pos + 1).saturating_sub(window);
        let tail = &bars.rows[start..=pos];
        if tail.is_empty() {
            return (false, Some("no volume history".to_string()));
        }
        let adv = tail.iter().map(|bar| bar.close * bar.volume).sum::<f64>() / tail.len() as f64;
        if !adv.is_finite() || adv < min_adv {
            return (false, Some(format!("adv {adv:.0} < {min_adv}")));
        }
    }
    (true, None)
}

fn resolve_universe(cfg: &BacktestConfig) -> anyhow::Result<(Vec<String>, Vec<String>)> {
    let mut warnings = Vec::new();
    let mut tickers = if let Some(tickers) = &cfg.tickers {
        tickers.clone()
    } else if let Some(path) = &cfg.universe_file {
        fs::read_to_string(path)?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(ToString::to_string)
            .collect()
    } else {
        anyhow::bail!(
            "No universe provided: pass --tickers or --universe-file. The TradingView current-screener fallback was removed because it injects survivorship bias."
        );
    };
    if cfg.max_universe > 0 && tickers.len() > cfg.max_universe {
        warnings.push(format!(
            "capped universe from {} to {} tickers",
            tickers.len(),
            cfg.max_universe
        ));
        tickers.truncate(cfg.max_universe);
    }
    Ok((tickers, warnings))
}

fn select_candidates(
    bars_by_ticker: &BarsByTicker,
    entry_ast: &Node,
    as_of: NaiveDate,
    top_n: usize,
    lookback_required: usize,
    cfg: &BacktestConfig,
) -> anyhow::Result<(Vec<SelectionRow>, Vec<String>)> {
    let mut rows = Vec::new();
    let mut warnings = Vec::new();
    let mut filtered_count = 0;
    let pool_limit = (top_n * cfg.reserve_multiple.max(1)).max(top_n);
    for (ticker, bars) in bars_by_ticker {
        if bars.is_empty() {
            warnings.push(format!("no data: {ticker}"));
            continue;
        }
        let Some(pos) = bars.position_on_or_before(as_of) else {
            warnings.push(format!("insufficient lookback (0 bars): {ticker}"));
            continue;
        };
        if pos + 1 < lookback_required + 1 {
            warnings.push(format!(
                "insufficient lookback ({} bars): {ticker}",
                pos + 1
            ));
            continue;
        }
        let (passes, _) = passes_entry_filters(bars, as_of, cfg);
        if !passes {
            filtered_count += 1;
            continue;
        }
        let history = bars.slice_through(pos);
        let signal = match evaluate(entry_ast, &history) {
            Ok(signal) => signal,
            Err(err) => {
                warnings.push(format!("entry eval failed: {ticker}: {err}"));
                continue;
            }
        };
        if signal
            .last()
            .is_none_or(|value| *value == 0.0 || value.is_nan())
        {
            continue;
        }
        let last = &bars.rows[pos];
        rows.push(SelectionRow {
            ticker: ticker.clone(),
            signal_date: None,
            as_of_close: last.close,
            as_of_volume: last.volume,
            as_of_dollar_vol: last.close * last.volume,
            rank: 0,
            role: String::new(),
        });
    }
    if filtered_count > 0 {
        warnings.push(format!(
            "filtered {filtered_count} tickers on price/liquidity filters"
        ));
    }
    rows.sort_by(|a, b| {
        b.as_of_dollar_vol
            .partial_cmp(&a.as_of_dollar_vol)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rows.truncate(pool_limit);
    for (i, row) in rows.iter_mut().enumerate() {
        row.rank = i + 1;
        row.role = if i < top_n { "active" } else { "reserve" }.to_string();
    }
    Ok((rows, warnings))
}

fn eligible_reserve_signal_idx(
    bars: &Bars,
    exit_day: NaiveDate,
    cfg: &BacktestConfig,
    entry_ast: &Node,
    lookback: usize,
) -> Option<usize> {
    let pos = bars.position_on_or_before(exit_day)?;
    if pos + 1 < lookback + 1 {
        return None;
    }
    if !passes_entry_filters(bars, exit_day, cfg).0 {
        return None;
    }
    let history = bars.slice_through(pos);
    let signal = evaluate(entry_ast, &history).ok()?;
    if signal
        .last()
        .is_some_and(|value| *value != 0.0 && !value.is_nan())
    {
        Some(pos)
    } else {
        None
    }
}

fn close_slot_at_day(
    state: &mut SlotState,
    bars: &Bars,
    day: NaiveDate,
    cfg: &BacktestConfig,
    portfolio: &mut Portfolio,
) -> anyhow::Result<bool> {
    let Some(idx) = bars.position(day) else {
        return Ok(false);
    };
    if idx < state.entry_idx + 1 {
        return Ok(false);
    }
    maybe_credit_dividends(portfolio, state, bars, idx, cfg);
    fire_partial_exits_at_bar(state, bars, idx, cfg, portfolio)?;
    if portfolio.get_position(&state.ticker).is_none() {
        return Ok(true);
    }
    if let Some((fill, reason)) = check_exit_at_bar(state, bars, idx, cfg) {
        portfolio.close(&state.ticker, day, fill, reason, cfg.commission_bps)?;
        return Ok(true);
    }
    Ok(false)
}

struct EventDrivenSimCtx<'a> {
    actives: &'a [SelectionRow],
    reserves: &'a [SelectionRow],
    bars_by_tv: &'a BarsByTicker,
    as_of: NaiveDate,
    cfg: &'a BacktestConfig,
    entry_ast: &'a Node,
    exit_ast: Option<&'a Node>,
    lookback: usize,
}

fn run_event_driven_sim(
    portfolio: &mut Portfolio,
    ctx: EventDrivenSimCtx<'_>,
    warnings: &mut Vec<String>,
) -> anyhow::Result<()> {
    let EventDrivenSimCtx {
        actives,
        reserves,
        bars_by_tv,
        as_of,
        cfg,
        entry_ast,
        exit_ast,
        lookback,
    } = ctx;
    let mut slot_states: BTreeMap<usize, Option<SlotState>> = BTreeMap::new();
    let mut slot_bars: BTreeMap<usize, Bars> = BTreeMap::new();
    let mut reentries_left: BTreeMap<usize, usize> = BTreeMap::new();
    let mut pending_reentry: BTreeMap<usize, String> = BTreeMap::new();

    for (slot_id, row) in actives.iter().enumerate() {
        let Some(bars) = bars_by_tv.get(&row.ticker) else {
            warnings.push(format!("no data during sim: {}", row.ticker));
            slot_states.insert(slot_id, None);
            continue;
        };
        let Some(signal_idx) = bars.position_on_or_before(as_of) else {
            warnings.push(format!("no history at as_of: {}", row.ticker));
            slot_states.insert(slot_id, None);
            continue;
        };
        let (state, warn) =
            make_slot_state(&row.ticker, bars, signal_idx, cfg, exit_ast, row.rank)?;
        let Some(state) = state else {
            if let Some(warn) = warn {
                warnings.push(format!("{}: {warn}", row.ticker));
            }
            slot_states.insert(slot_id, None);
            continue;
        };
        portfolio.assign(&row.ticker, row.rank, cfg.as_of);
        portfolio.open(
            &row.ticker,
            state.entry_date,
            state.entry_fill,
            cfg.commission_bps,
            raise_if_position_exists(cfg),
        )?;
        slot_bars.insert(slot_id, bars.clone());
        reentries_left.insert(
            slot_id,
            if cfg.allow_reentry {
                cfg.max_reentries
            } else {
                0
            },
        );
        slot_states.insert(slot_id, Some(state));
    }

    let mut taken: BTreeSet<String> = slot_states
        .values()
        .filter_map(|state| state.as_ref().map(|s| s.ticker.clone()))
        .collect();
    let mut reserve_queue = reserves.to_vec();
    let horizon_end = as_of + Duration::days((cfg.hold * 3 + 60).max(90) as i64);
    let mut master_dates = BTreeSet::new();
    for bars in bars_by_tv.values() {
        for bar in &bars.rows {
            if bar.date > as_of && bar.date <= horizon_end {
                master_dates.insert(bar.date);
            }
        }
    }

    for day in master_dates {
        let pending = pending_reentry.clone();
        for (slot_id, ticker) in pending {
            let Some(slot_frame) = slot_bars.get(&slot_id) else {
                pending_reentry.remove(&slot_id);
                continue;
            };
            let Some(signal_idx) =
                eligible_reserve_signal_idx(slot_frame, day, cfg, entry_ast, lookback)
            else {
                continue;
            };
            let rank = portfolio
                .closed_trades()
                .iter()
                .find(|trade| trade.ticker == ticker)
                .map(|trade| trade.rank)
                .unwrap_or(0);
            let (state, warn) =
                make_slot_state(&ticker, slot_frame, signal_idx, cfg, exit_ast, rank)?;
            let Some(state) = state else {
                if let Some(warn) = warn {
                    warnings.push(format!("{ticker} re-entry: {warn}"));
                }
                pending_reentry.remove(&slot_id);
                continue;
            };
            portfolio.assign(&ticker, rank, day);
            portfolio.open(
                &ticker,
                state.entry_date,
                state.entry_fill,
                cfg.commission_bps,
                raise_if_position_exists(cfg),
            )?;
            slot_states.insert(slot_id, Some(state));
            pending_reentry.remove(&slot_id);
        }

        let mut freed = Vec::new();
        let slot_ids: Vec<usize> = slot_states.keys().copied().collect();
        for slot_id in slot_ids {
            let Some(Some(state)) = slot_states.get_mut(&slot_id) else {
                continue;
            };
            let bars = slot_bars.get(&slot_id).expect("slot bars exist");
            if close_slot_at_day(state, bars, day, cfg, portfolio)? {
                let ticker = state.ticker.clone();
                slot_states.insert(slot_id, None);
                freed.push(slot_id);
                if cfg.allow_reentry && reentries_left.get(&slot_id).copied().unwrap_or(0) > 0 {
                    *reentries_left.entry(slot_id).or_default() -= 1;
                    pending_reentry.insert(slot_id, ticker);
                }
            }
        }

        if !cfg.reinvest || freed.is_empty() {
            continue;
        }
        for slot_id in freed {
            if pending_reentry.contains_key(&slot_id) {
                continue;
            }
            while let Some(reserve) = reserve_queue.first().cloned() {
                reserve_queue.remove(0);
                if taken.contains(&reserve.ticker) {
                    continue;
                }
                let Some(reserve_bars) = bars_by_tv.get(&reserve.ticker) else {
                    continue;
                };
                let Some(signal_idx) =
                    eligible_reserve_signal_idx(reserve_bars, day, cfg, entry_ast, lookback)
                else {
                    continue;
                };
                let (state, warn) = make_slot_state(
                    &reserve.ticker,
                    reserve_bars,
                    signal_idx,
                    cfg,
                    exit_ast,
                    reserve.rank,
                )?;
                let Some(state) = state else {
                    if let Some(warn) = warn {
                        warnings.push(format!("{} reserve: {warn}", reserve.ticker));
                    }
                    continue;
                };
                portfolio.assign(&reserve.ticker, reserve.rank, day);
                portfolio.open(
                    &reserve.ticker,
                    state.entry_date,
                    state.entry_fill,
                    cfg.commission_bps,
                    raise_if_position_exists(cfg),
                )?;
                taken.insert(reserve.ticker.clone());
                slot_bars.insert(slot_id, reserve_bars.clone());
                slot_states.insert(slot_id, Some(state));
                break;
            }
        }
    }
    Ok(())
}

fn fetch_benchmark(
    benchmark: &str,
    start: NaiveDate,
    end: NaiveDate,
    fetcher: &dyn PriceFetcher,
) -> anyhow::Result<Vec<(NaiveDate, f64)>> {
    let panel = fetcher.fetch(&[benchmark.to_string()], start, end)?;
    Ok(panel
        .get(benchmark)
        .map(|bars| bars.rows.iter().map(|bar| (bar.date, bar.close)).collect())
        .unwrap_or_default())
}

pub fn run_backtest(
    cfg: BacktestConfig,
    fetcher: &dyn PriceFetcher,
) -> anyhow::Result<BacktestResult> {
    let mut warnings = Vec::new();
    let entry_ast = parse(&cfg.entry_expr)?;
    let exit_ast = match &cfg.exit_expr {
        Some(expr) if !expr.trim().is_empty() => Some(parse(expr)?),
        _ => None,
    };
    let mut lookback = required_lookback(&entry_ast);
    if let Some(exit_ast) = &exit_ast {
        lookback = lookback.max(required_lookback(exit_ast));
    }

    let (tv_symbols, universe_warnings) = resolve_universe(&cfg)?;
    warnings.extend(universe_warnings);
    let yf_by_tv: BTreeMap<String, String> = tv_symbols
        .iter()
        .map(|tv| (tv.clone(), tv_to_yf(tv, &cfg.market)))
        .collect();
    let mut yf_symbols: Vec<String> = yf_by_tv.values().cloned().collect();
    yf_symbols.push(cfg.benchmark.clone());
    yf_symbols.sort();
    yf_symbols.dedup();

    let start = cfg.as_of - Duration::days((lookback * 2 + 30).max(365) as i64);
    let end = cfg.as_of + Duration::days((cfg.hold * 2 + 30) as i64);
    let price_panel = fetcher.fetch(&yf_symbols, start, end)?;
    let bars_by_tv: BarsByTicker = tv_symbols
        .iter()
        .map(|tv| {
            let yf = yf_by_tv.get(tv).expect("mapped");
            (tv.clone(), price_panel.get(yf).cloned().unwrap_or_default())
        })
        .collect();

    let (selection, selection_warnings) =
        select_candidates(&bars_by_tv, &entry_ast, cfg.as_of, cfg.top, lookback, &cfg)?;
    warnings.extend(selection_warnings);

    if selection.is_empty() {
        let calendar = business_days(cfg.as_of, cfg.as_of + Duration::days((cfg.hold * 2) as i64));
        let equity = calendar
            .into_iter()
            .map(|day| (day, cfg.initial_capital))
            .collect::<Vec<_>>();
        let benchmark = fetch_benchmark(&cfg.benchmark, start, end, fetcher)?;
        let metrics = compute_metrics(&equity, &benchmark, &[], cfg.top.max(1), 1);
        return Ok(BacktestResult {
            config: cfg,
            trades: Vec::new(),
            equity_curve: equity,
            benchmark_curve: benchmark,
            metrics,
            warnings,
            selection,
        });
    }

    let actives = selection
        .iter()
        .filter(|row| row.role == "active")
        .cloned()
        .collect::<Vec<_>>();
    let reserves = selection
        .iter()
        .filter(|row| row.role == "reserve")
        .cloned()
        .collect::<Vec<_>>();
    let slot_count = cfg.top.max(actives.len()).max(1);
    let mut portfolio = Portfolio::new(cfg.initial_capital, slot_count)?;
    run_event_driven_sim(
        &mut portfolio,
        EventDrivenSimCtx {
            actives: &actives,
            reserves: &reserves,
            bars_by_tv: &bars_by_tv,
            as_of: cfg.as_of,
            cfg: &cfg,
            entry_ast: &entry_ast,
            exit_ast: exit_ast.as_ref(),
            lookback,
        },
        &mut warnings,
    )?;
    let trades = portfolio.closed_trades();
    let mut date_set = BTreeSet::from([cfg.as_of]);
    for trade in &trades {
        if let Some(frame) = bars_by_tv.get(&trade.ticker) {
            for bar in &frame.between(trade.entry_date, trade.exit_date).rows {
                date_set.insert(bar.date);
            }
        }
    }
    if date_set.is_empty() {
        date_set.extend(business_days(
            cfg.as_of,
            cfg.as_of + Duration::days((cfg.hold * 2) as i64),
        ));
    }
    let calendar = date_set.into_iter().collect::<Vec<_>>();
    let equity = build_equity_curve(&calendar, &trades, &bars_by_tv, cfg.initial_capital);
    let benchmark = fetch_benchmark(&cfg.benchmark, start, end, fetcher)?;
    let metrics = compute_metrics(&equity, &benchmark, &trades, slot_count, 1);
    Ok(BacktestResult {
        config: cfg,
        trades,
        equity_curve: equity,
        benchmark_curve: benchmark,
        metrics,
        warnings,
        selection,
    })
}

fn candidate_rows_for_day(
    day: NaiveDate,
    bars_by_tv: &BarsByTicker,
    entry_ast: &Node,
    lookback: usize,
    cfg: &BacktestConfig,
    exclude: &BTreeSet<String>,
    warnings: &mut Vec<String>,
) -> Vec<SelectionRow> {
    let mut rows = Vec::new();
    for (ticker, bars) in bars_by_tv {
        if exclude.contains(ticker) {
            continue;
        }
        let Some(pos) = bars.position_on_or_before(day) else {
            continue;
        };
        if pos + 1 < lookback + 1 || pos + 1 >= bars.len() {
            continue;
        }
        if !passes_entry_filters(bars, day, cfg).0 {
            continue;
        }
        let history = bars.slice_through(pos);
        let signal = match evaluate(entry_ast, &history) {
            Ok(signal) => signal,
            Err(err) => {
                warnings.push(format!("entry eval failed: {ticker}: {err}"));
                continue;
            }
        };
        if signal
            .last()
            .is_none_or(|value| *value == 0.0 || value.is_nan())
        {
            continue;
        }
        let bar = &bars.rows[pos];
        rows.push(SelectionRow {
            ticker: ticker.clone(),
            signal_date: Some(day),
            as_of_close: bar.close,
            as_of_volume: bar.volume,
            as_of_dollar_vol: bar.close * bar.volume,
            rank: 0,
            role: "active".to_string(),
        });
    }
    rows.sort_by(|a, b| {
        b.as_of_dollar_vol
            .partial_cmp(&a.as_of_dollar_vol)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for (i, row) in rows.iter_mut().enumerate() {
        row.rank = i + 1;
    }
    rows
}

pub fn run_rolling_backtest(
    cfg: BacktestConfig,
    fetcher: &dyn PriceFetcher,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> anyhow::Result<BacktestResult> {
    if end_date < start_date {
        anyhow::bail!("end_date must be >= start_date");
    }
    let mut warnings = Vec::new();
    let entry_ast = parse(&cfg.entry_expr)?;
    let exit_ast = match &cfg.exit_expr {
        Some(expr) if !expr.trim().is_empty() => Some(parse(expr)?),
        _ => None,
    };
    let mut lookback = required_lookback(&entry_ast);
    if let Some(exit_ast) = &exit_ast {
        lookback = lookback.max(required_lookback(exit_ast));
    }
    let (tv_symbols, universe_warnings) = resolve_universe(&cfg)?;
    warnings.extend(universe_warnings);
    let yf_by_tv: BTreeMap<String, String> = tv_symbols
        .iter()
        .map(|tv| (tv.clone(), tv_to_yf(tv, &cfg.market)))
        .collect();
    let mut yf_symbols: Vec<String> = yf_by_tv.values().cloned().collect();
    yf_symbols.push(cfg.benchmark.clone());
    yf_symbols.sort();
    yf_symbols.dedup();
    let fetch_start = start_date - Duration::days((lookback * 3 + 30).max(365) as i64);
    let price_panel = fetcher.fetch(&yf_symbols, fetch_start, end_date)?;
    let bars_by_tv: BarsByTicker = tv_symbols
        .iter()
        .map(|tv| {
            let yf = yf_by_tv.get(tv).expect("mapped");
            (tv.clone(), price_panel.get(yf).cloned().unwrap_or_default())
        })
        .collect();

    let mut master_dates = BTreeSet::new();
    for bars in bars_by_tv.values() {
        for bar in &bars.rows {
            if bar.date >= start_date && bar.date <= end_date {
                master_dates.insert(bar.date);
            }
        }
    }
    if master_dates.is_empty() {
        let calendar = business_days(start_date, end_date);
        let equity = calendar
            .iter()
            .map(|day| (*day, cfg.initial_capital))
            .collect::<Vec<_>>();
        let benchmark = fetch_benchmark(&cfg.benchmark, fetch_start, end_date, fetcher)?;
        let mut metrics = compute_metrics(&equity, &benchmark, &[], cfg.top.max(1), 1);
        metrics.insert("unique_tickers".to_string(), 0.0);
        warnings.push("no trading days with price data in rolling window".to_string());
        return Ok(BacktestResult {
            config: cfg,
            trades: Vec::new(),
            equity_curve: equity,
            benchmark_curve: benchmark,
            metrics,
            warnings,
            selection: Vec::new(),
        });
    }

    let mut portfolio = Portfolio::new(cfg.initial_capital, cfg.top.max(1))?;
    let mut slot_states: BTreeMap<usize, Option<SlotState>> =
        (0..cfg.top.max(1)).map(|slot| (slot, None)).collect();
    let mut slot_bars: BTreeMap<usize, Bars> = BTreeMap::new();
    let mut entries_by_ticker: BTreeMap<String, usize> = BTreeMap::new();
    let mut selection_rows = Vec::new();

    for day in master_dates.iter().copied() {
        let mut free_slots = Vec::new();
        for slot_id in 0..cfg.top.max(1) {
            match slot_states.get_mut(&slot_id) {
                Some(Some(state)) => {
                    let bars = slot_bars.get(&slot_id).expect("slot bars exist");
                    if close_slot_at_day(state, bars, day, &cfg, &mut portfolio)? {
                        slot_states.insert(slot_id, None);
                        free_slots.push(slot_id);
                    }
                }
                _ => free_slots.push(slot_id),
            }
        }
        if free_slots.is_empty() {
            continue;
        }
        let mut exclude = tickers_at_concurrency_cap(&slot_states, &cfg);
        for (ticker, entries) in &entries_by_ticker {
            let entry_cap = if cfg.allow_reentry {
                cfg.max_reentries.saturating_add(1)
            } else {
                1
            };
            if *entries >= entry_cap {
                exclude.insert(ticker.clone());
            }
        }
        let mut candidates = candidate_rows_for_day(
            day,
            &bars_by_tv,
            &entry_ast,
            lookback,
            &cfg,
            &exclude,
            &mut warnings,
        );
        if candidates.is_empty() {
            continue;
        }
        for slot_id in free_slots {
            while let Some(row) = candidates.first().cloned() {
                candidates.remove(0);
                if !ticker_below_concurrency_cap(&slot_states, &row.ticker, &cfg) {
                    continue;
                }
                let Some(bars) = bars_by_tv.get(&row.ticker) else {
                    continue;
                };
                let signal_idx = bars.position_on_or_before(day).expect("candidate has bar");
                let (state, warn) = make_slot_state(
                    &row.ticker,
                    bars,
                    signal_idx,
                    &cfg,
                    exit_ast.as_ref(),
                    row.rank,
                )?;
                let Some(state) = state else {
                    if let Some(warn) = warn {
                        warnings.push(format!("{}: {warn}", row.ticker));
                    }
                    continue;
                };
                if state.entry_date > end_date {
                    continue;
                }
                portfolio.assign(&row.ticker, row.rank, day);
                portfolio.open(
                    &row.ticker,
                    state.entry_date,
                    state.entry_fill,
                    cfg.commission_bps,
                    raise_if_position_exists(&cfg),
                )?;
                *entries_by_ticker.entry(row.ticker.clone()).or_default() += 1;
                slot_bars.insert(slot_id, bars.clone());
                slot_states.insert(slot_id, Some(state));
                selection_rows.push(row);
                break;
            }
        }
    }

    for (slot_id, state) in slot_states.iter_mut() {
        let Some(state) = state else {
            continue;
        };
        let Some(bars) = slot_bars.get(slot_id) else {
            continue;
        };
        let tail = bars.between(state.entry_date, end_date);
        let Some(last) = tail.rows.last() else {
            continue;
        };
        let fill = slip(
            last.close,
            Side::Sell,
            &cfg,
            state.adv_shares,
            state.sigma_daily,
        );
        portfolio.close(
            &state.ticker,
            last.date,
            fill,
            ExitReason::Eod,
            cfg.commission_bps,
        )?;
    }
    let trades = portfolio.closed_trades();
    let mut date_set = master_dates;
    for trade in &trades {
        if let Some(frame) = bars_by_tv.get(&trade.ticker) {
            for bar in &frame.between(trade.entry_date, trade.exit_date).rows {
                date_set.insert(bar.date);
            }
        }
    }
    let calendar = date_set.into_iter().collect::<Vec<_>>();
    let equity = build_equity_curve(&calendar, &trades, &bars_by_tv, cfg.initial_capital);
    let benchmark = fetch_benchmark(&cfg.benchmark, fetch_start, end_date, fetcher)?;
    let mut metrics = compute_metrics(&equity, &benchmark, &trades, cfg.top.max(1), 1);
    metrics.insert(
        "unique_tickers".to_string(),
        trades
            .iter()
            .map(|trade| trade.ticker.clone())
            .collect::<BTreeSet<_>>()
            .len() as f64,
    );
    Ok(BacktestResult {
        config: cfg,
        trades,
        equity_curve: equity,
        benchmark_curve: benchmark,
        metrics,
        warnings,
        selection: selection_rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backtester::models::BacktestConfig;
    use crate::backtester::slippage::SlippageModel;
    use crate::data::{Bar, Bars};
    use approx::assert_relative_eq;
    use chrono::Duration;

    struct StubPriceFetcher {
        panel: PricePanel,
    }

    impl PriceFetcher for StubPriceFetcher {
        fn fetch(
            &self,
            tickers: &[String],
            _start: NaiveDate,
            _end: NaiveDate,
        ) -> anyhow::Result<PricePanel> {
            Ok(tickers
                .iter()
                .map(|ticker| {
                    (
                        ticker.clone(),
                        self.panel.get(ticker).cloned().unwrap_or_default(),
                    )
                })
                .collect())
        }
    }

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, day).unwrap()
    }

    fn make_bars(n: usize) -> Bars {
        let start = d(1);
        Bars::new(
            (0..n)
                .map(|i| {
                    let px = 100.0 + i as f64;
                    Bar {
                        date: start + Duration::days(i as i64),
                        open: px,
                        high: px + 1.0,
                        low: px - 1.0,
                        close: px + 0.2,
                        volume: 100_000.0,
                        adj_close: None,
                        dividend: None,
                        extra: BTreeMap::new(),
                    }
                })
                .collect(),
        )
    }

    fn make_flat_bars(n: usize, price: f64) -> Bars {
        let start = d(1);
        Bars::new(
            (0..n)
                .map(|i| Bar {
                    date: start + Duration::days(i as i64),
                    open: price,
                    high: price + 1.0,
                    low: price - 1.0,
                    close: price,
                    volume: 100_000.0,
                    adj_close: None,
                    dividend: None,
                    extra: BTreeMap::new(),
                })
                .collect(),
        )
    }

    fn set_bar(bars: &mut Bars, idx: usize, open: f64, high: f64, low: f64, close: f64) {
        bars.rows[idx].open = open;
        bars.rows[idx].high = high;
        bars.rows[idx].low = low;
        bars.rows[idx].close = close;
    }

    fn set_extra(bars: &mut Bars, idx: usize, name: &str, value: f64) {
        bars.rows[idx].extra.insert(name.to_string(), value);
    }

    fn trend_bars(n: usize, start_px: f64, end_px: f64, volume: f64) -> Bars {
        let start = d(1);
        Bars::new(
            (0..n)
                .map(|i| {
                    let t = if n > 1 {
                        i as f64 / (n - 1) as f64
                    } else {
                        0.0
                    };
                    let close = start_px + (end_px - start_px) * t;
                    let open = if i == 0 {
                        close - 1.0
                    } else {
                        start_px + (end_px - start_px) * (i - 1) as f64 / (n - 1) as f64
                    };
                    Bar {
                        date: start + Duration::days(i as i64),
                        open,
                        high: open.max(close) + 1.0,
                        low: open.min(close) - 1.0,
                        close,
                        volume,
                        adj_close: None,
                        dividend: None,
                        extra: BTreeMap::new(),
                    }
                })
                .collect(),
        )
    }

    fn cfg() -> BacktestConfig {
        BacktestConfig {
            market: "us".to_string(),
            as_of: d(4),
            hold: 5,
            top: 1,
            entry_expr: "close > sma(close, 3)".to_string(),
            exit_expr: None,
            stop_loss: None,
            take_profit: None,
            trailing_stop: None,
            slippage_bps: 0.0,
            commission_bps: 0.0,
            initial_capital: 100_000.0,
            benchmark: "SPY".to_string(),
            strategy_name: None,
            tickers: Some(vec!["AAA".to_string()]),
            universe_file: None,
            max_universe: 200,
            min_price: None,
            min_avg_dollar_volume: None,
            avg_dollar_volume_window: 20,
            reserve_multiple: 3,
            reinvest: true,
            slippage_model: SlippageModel::Fixed { bps: 0.0 },
            gap_fills: true,
            entry_order_type: EntryOrderType::Moo,
            entry_limit_bps: None,
            allow_reentry: false,
            max_reentries: 0,
            max_concurrent_per_ticker: 1,
            partial_exits: Vec::new(),
            price_adjustment: PriceAdjustment::Full,
        }
    }

    #[test]
    fn entry_fills_next_day_open() {
        let bars = make_bars(10);
        let outcome = simulate_ticker(&bars, 3, &cfg(), None).unwrap();
        let trade = outcome.trade.unwrap();
        assert_eq!(trade.entry_date, bars.rows[4].date);
        assert_relative_eq!(trade.entry_price, bars.rows[4].open);
    }

    #[test]
    fn no_post_signal_bar_warns() {
        let bars = make_bars(5);
        let outcome = simulate_ticker(&bars, 4, &cfg(), None).unwrap();
        assert!(outcome.trade.is_none());
        assert!(outcome.warning.unwrap().contains("no post-signal"));
    }

    #[test]
    fn stop_loss_triggers_from_low() {
        let mut bars = make_bars(10);
        bars.rows[4].open = 100.0;
        bars.rows[4].high = 100.5;
        bars.rows[4].low = 100.0;
        bars.rows[5].open = 100.2;
        bars.rows[5].low = 89.0;
        let mut cfg = cfg();
        cfg.hold = 10;
        cfg.stop_loss = Some(0.05);
        let trade = simulate_ticker(&bars, 3, &cfg, None)
            .unwrap()
            .trade
            .unwrap();
        assert_eq!(trade.exit_reason, ExitReason::Stop);
        assert_relative_eq!(trade.exit_price, 95.0);
        assert_eq!(trade.exit_date, bars.rows[5].date);
    }

    #[test]
    fn same_bar_stop_and_target_stop_wins() {
        let mut bars = make_bars(10);
        bars.rows[4].open = 100.0;
        bars.rows[4].high = 100.0;
        bars.rows[4].low = 100.0;
        bars.rows[5].open = 100.0;
        bars.rows[5].high = 130.0;
        bars.rows[5].low = 85.0;
        let mut cfg = cfg();
        cfg.hold = 10;
        cfg.stop_loss = Some(0.05);
        cfg.take_profit = Some(0.10);
        let trade = simulate_ticker(&bars, 3, &cfg, None)
            .unwrap()
            .trade
            .unwrap();
        assert_eq!(trade.exit_reason, ExitReason::Stop);
    }

    #[test]
    fn take_profit_triggers_from_high() {
        let mut bars = make_bars(10);
        set_bar(&mut bars, 4, 100.0, 100.5, 99.8, 100.2);
        set_bar(&mut bars, 5, 100.2, 130.0, 100.0, 110.0);
        let mut cfg = cfg();
        cfg.hold = 10;
        cfg.take_profit = Some(0.10);
        let trade = simulate_ticker(&bars, 3, &cfg, None)
            .unwrap()
            .trade
            .unwrap();
        assert_eq!(trade.exit_reason, ExitReason::Target);
        assert_relative_eq!(trade.exit_price, 110.0);
    }

    #[test]
    fn same_bar_trail_and_target_trail_wins() {
        let mut bars = make_bars(10);
        set_bar(&mut bars, 4, 100.0, 100.5, 99.5, 100.0);
        set_bar(&mut bars, 5, 100.0, 109.0, 99.8, 109.0);
        set_bar(&mut bars, 6, 109.0, 115.0, 95.0, 100.0);
        let mut cfg = cfg();
        cfg.hold = 10;
        cfg.trailing_stop = Some(0.10);
        cfg.take_profit = Some(0.10);
        let trade = simulate_ticker(&bars, 3, &cfg, None)
            .unwrap()
            .trade
            .unwrap();
        assert_eq!(trade.exit_reason, ExitReason::Trail);
        assert_relative_eq!(trade.exit_price, 109.0 * 0.9);
    }

    #[test]
    fn trailing_stop_tracks_peak() {
        let mut bars = make_bars(10);
        set_bar(&mut bars, 4, 100.0, 100.5, 99.5, 100.0);
        set_bar(&mut bars, 5, 100.0, 120.0, 99.8, 118.0);
        set_bar(&mut bars, 6, 118.0, 118.5, 100.0, 101.0);
        let mut cfg = cfg();
        cfg.hold = 10;
        cfg.trailing_stop = Some(0.10);
        let trade = simulate_ticker(&bars, 3, &cfg, None)
            .unwrap()
            .trade
            .unwrap();
        assert_eq!(trade.exit_reason, ExitReason::Trail);
        assert_relative_eq!(trade.exit_price, 120.0 * 0.9);
    }

    #[test]
    fn exit_expression_triggers_at_close() {
        let mut bars = make_bars(15);
        for i in 5..7 {
            bars.rows[i].close = bars.rows[i].open + 1.0;
        }
        bars.rows[7].close = bars.rows[7].open - 2.0;
        let ast = crate::pine::parse("close < open").unwrap();
        let mut cfg = cfg();
        cfg.hold = 20;
        let trade = simulate_ticker(&bars, 3, &cfg, Some(&ast))
            .unwrap()
            .trade
            .unwrap();
        assert_eq!(trade.exit_reason, ExitReason::ExitExpr);
        assert_eq!(trade.exit_date, bars.rows[7].date);
        assert_relative_eq!(trade.exit_price, bars.rows[7].close);
    }

    #[test]
    fn slippage_reduces_return_vs_zero_slip() {
        let bars = make_bars(20);
        let mut slipped_cfg = cfg();
        slipped_cfg.hold = 5;
        slipped_cfg.slippage_model = SlippageModel::Fixed { bps: 50.0 };
        let mut zero_cfg = cfg();
        zero_cfg.hold = 5;
        let zero = simulate_ticker(&bars, 3, &zero_cfg, None)
            .unwrap()
            .trade
            .unwrap();
        let slipped = simulate_ticker(&bars, 3, &slipped_cfg, None)
            .unwrap()
            .trade
            .unwrap();
        assert!(slipped.entry_price > zero.entry_price);
        assert!(slipped.exit_price < zero.exit_price);
    }

    #[test]
    fn commission_reduces_realized_pnl() {
        let bars = make_bars(20);
        let trade = simulate_ticker(&bars, 3, &cfg(), None)
            .unwrap()
            .trade
            .unwrap();
        let mut no_commission = Portfolio::new(100_000.0, 1).unwrap();
        no_commission.assign("AAA", 1, bars.rows[3].date);
        no_commission
            .open("AAA", trade.entry_date, trade.entry_price, 0.0, true)
            .unwrap();
        let clean = no_commission
            .close(
                "AAA",
                trade.exit_date,
                trade.exit_price,
                trade.exit_reason,
                0.0,
            )
            .unwrap();

        let mut commissioned = Portfolio::new(100_000.0, 1).unwrap();
        commissioned.assign("AAA", 1, bars.rows[3].date);
        commissioned
            .open("AAA", trade.entry_date, trade.entry_price, 50.0, true)
            .unwrap();
        let costly = commissioned
            .close(
                "AAA",
                trade.exit_date,
                trade.exit_price,
                trade.exit_reason,
                50.0,
            )
            .unwrap();
        assert!(costly.pnl < clean.pnl);
    }

    #[test]
    fn historical_backtest_selects_and_trades() {
        let aaa = make_bars(12);
        let spy = make_bars(12);
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([("AAA".to_string(), aaa.clone()), ("SPY".to_string(), spy)]),
        };
        let result = run_backtest(cfg(), &fetcher).unwrap();
        assert_eq!(result.selection[0].ticker, "AAA");
        assert_eq!(result.trades[0].ticker, "AAA");
    }

    #[test]
    fn rolling_backtest_generates_signal_after_window_start() {
        let mut aaa = make_flat_bars(30, 100.0);
        aaa.rows[10].close = 250.0;
        let spy = make_flat_bars(30, 400.0);
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([("AAA".to_string(), aaa.clone()), ("SPY".to_string(), spy)]),
        };
        let mut cfg = cfg();
        cfg.as_of = aaa.rows[20].date;
        cfg.hold = 20;
        cfg.entry_expr = "close > 200".to_string();
        let result =
            run_rolling_backtest(cfg, &fetcher, aaa.rows[0].date, aaa.rows[20].date).unwrap();
        assert_eq!(result.trades.len(), 1);
        assert_eq!(result.trades[0].signal_date, aaa.rows[10].date);
        assert_eq!(result.trades[0].entry_date, aaa.rows[11].date);
    }

    #[test]
    fn reserve_reallocation_fills_freed_slot() {
        let mut active = make_flat_bars(60, 100.0);
        let mut reserve = make_flat_bars(60, 100.0);
        let spy = make_flat_bars(60, 400.0);
        active.rows[39].close = 150.0;
        set_bar(&mut active, 40, 100.0, 101.0, 99.0, 100.0);
        set_bar(&mut active, 41, 99.0, 99.5, 90.0, 91.0);
        reserve.rows[39].close = 120.0;
        reserve.rows[41].close = 120.0;
        for row in &mut active.rows {
            row.volume = 1_000_000.0;
        }
        for row in &mut reserve.rows {
            row.volume = 100_000.0;
        }
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([
                ("ACTIVE".to_string(), active.clone()),
                ("RESERVE".to_string(), reserve.clone()),
                ("SPY".to_string(), spy),
            ]),
        };
        let mut cfg = cfg();
        cfg.as_of = active.rows[39].date;
        cfg.hold = 10;
        cfg.top = 1;
        cfg.tickers = Some(vec!["ACTIVE".to_string(), "RESERVE".to_string()]);
        cfg.entry_expr = "close > sma(close, 3)".to_string();
        cfg.stop_loss = Some(0.05);
        cfg.reserve_multiple = 3;
        cfg.reinvest = true;
        let result = run_backtest(cfg, &fetcher).unwrap();
        let active_trade = result
            .trades
            .iter()
            .find(|trade| trade.ticker == "ACTIVE")
            .unwrap();
        let reserve_trade = result
            .trades
            .iter()
            .find(|trade| trade.ticker == "RESERVE")
            .unwrap();
        assert_eq!(active_trade.exit_reason, ExitReason::Stop);
        assert!(reserve_trade.entry_date > active_trade.exit_date);
    }

    #[test]
    fn portfolio_equity_curve_keeps_cash_static_after_exit() {
        let mut bars_a = make_bars(20);
        let mut bars_b = make_bars(20);
        bars_a.rows[4].open = 100.0;
        bars_a.rows[6].close = 110.0;
        bars_b.rows[4].open = 50.0;
        let trade_a = simulate_ticker(
            &bars_a,
            3,
            &{
                let mut c = cfg();
                c.hold = 2;
                c
            },
            None,
        )
        .unwrap()
        .trade
        .unwrap();
        let trade_b = simulate_ticker(
            &bars_b,
            3,
            &{
                let mut c = cfg();
                c.hold = 15;
                c
            },
            None,
        )
        .unwrap()
        .trade
        .unwrap();
        let mut p_a = Portfolio::new(100_000.0, 2).unwrap();
        p_a.assign("AAA", 1, bars_a.rows[3].date);
        p_a.open("AAA", trade_a.entry_date, trade_a.entry_price, 0.0, true)
            .unwrap();
        let trade_a = p_a
            .close(
                "AAA",
                trade_a.exit_date,
                trade_a.exit_price,
                trade_a.exit_reason,
                0.0,
            )
            .unwrap();
        let mut p_b = Portfolio::new(100_000.0, 2).unwrap();
        p_b.assign("BBB", 2, bars_b.rows[3].date);
        p_b.open("BBB", trade_b.entry_date, trade_b.entry_price, 0.0, true)
            .unwrap();
        let trade_b = p_b
            .close(
                "BBB",
                trade_b.exit_date,
                trade_b.exit_price,
                trade_b.exit_reason,
                0.0,
            )
            .unwrap();
        let panel = BTreeMap::from([
            ("AAA".to_string(), bars_a.clone()),
            ("BBB".to_string(), bars_b.clone()),
        ]);
        let calendar = bars_a.dates();
        let equity = build_equity_curve(
            &calendar,
            &[trade_a.clone(), trade_b.clone()],
            &panel,
            100_000.0,
        );
        let after_a = equity
            .iter()
            .find(|(day, _)| *day > trade_a.exit_date && *day <= trade_b.exit_date)
            .unwrap();
        let b_close = bars_b
            .rows
            .iter()
            .find(|bar| bar.date == after_a.0)
            .unwrap()
            .close;
        let expected_cash =
            100_000.0 - trade_a.entry_cost - trade_b.entry_cost + trade_a.exit_value;
        assert_relative_eq!(after_a.1, expected_cash + trade_b.shares * b_close);
    }

    #[test]
    fn allow_reentry_false_preserves_single_historical_trade() {
        let mut bars = make_flat_bars(30, 100.0);
        set_bar(&mut bars, 5, 100.0, 100.5, 80.0, 85.0);
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([
                ("AAA".to_string(), bars.clone()),
                ("SPY".to_string(), bars.clone()),
            ]),
        };
        let mut cfg = cfg();
        cfg.as_of = bars.rows[3].date;
        cfg.hold = 20;
        cfg.entry_expr = "close > 0".to_string();
        cfg.stop_loss = Some(0.05);
        cfg.allow_reentry = false;
        let result = run_backtest(cfg, &fetcher).unwrap();
        assert_eq!(
            result.trades.iter().filter(|t| t.ticker == "AAA").count(),
            1
        );
    }

    #[test]
    fn reentry_after_stop_fires_again_and_respects_cap() {
        let mut bars = make_flat_bars(40, 100.0);
        for idx in [5, 8, 11, 14, 17] {
            bars.rows[idx].low = 70.0;
            bars.rows[idx].close = 72.0;
        }
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([
                ("AAA".to_string(), bars.clone()),
                ("SPY".to_string(), bars.clone()),
            ]),
        };
        let mut cfg = cfg();
        cfg.as_of = bars.rows[3].date;
        cfg.hold = 3;
        cfg.entry_expr = "close > 0".to_string();
        cfg.stop_loss = Some(0.05);
        cfg.allow_reentry = true;
        cfg.max_reentries = 1;
        let result = run_backtest(cfg, &fetcher).unwrap();
        let aaa = result
            .trades
            .iter()
            .filter(|t| t.ticker == "AAA")
            .collect::<Vec<_>>();
        assert_eq!(aaa.len(), 2);
        assert_eq!(aaa[0].exit_reason, ExitReason::Stop);
        assert!(aaa[1].entry_date > aaa[0].exit_date);
    }

    #[test]
    fn partial_exit_closes_half_at_tier_and_runner_time_exits() {
        let mut bars = make_flat_bars(20, 100.0);
        set_bar(&mut bars, 4, 100.0, 101.0, 99.5, 100.0);
        set_bar(&mut bars, 5, 100.5, 106.0, 100.0, 104.0);
        for idx in 6..20 {
            set_bar(&mut bars, idx, 103.0, 103.5, 102.0, 103.0);
        }
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([
                ("AAA".to_string(), bars.clone()),
                ("SPY".to_string(), bars.clone()),
            ]),
        };
        let mut cfg = cfg();
        cfg.as_of = bars.rows[3].date;
        cfg.hold = 5;
        cfg.entry_expr = "close > 0".to_string();
        cfg.partial_exits = vec![(0.05, 0.5)];
        let result = run_backtest(cfg, &fetcher).unwrap();
        let aaa = result
            .trades
            .iter()
            .filter(|t| t.ticker == "AAA")
            .collect::<Vec<_>>();
        assert_eq!(aaa.len(), 2);
        assert_eq!(aaa[0].exit_reason, ExitReason::Target);
        assert_relative_eq!(aaa[0].exit_price, 105.0);
        assert_relative_eq!(aaa[0].entry_cost, aaa[1].entry_cost);
    }

    #[test]
    fn split_only_price_adjustment_credits_dividends() {
        let mut bars = make_flat_bars(20, 100.0);
        bars.rows[6].dividend = Some(1.0);
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([
                ("AAA".to_string(), bars.clone()),
                ("SPY".to_string(), bars.clone()),
            ]),
        };
        let mut full_cfg = cfg();
        full_cfg.as_of = bars.rows[3].date;
        full_cfg.entry_expr = "close > 0".to_string();
        full_cfg.price_adjustment = PriceAdjustment::Full;
        let full = run_backtest(full_cfg, &fetcher).unwrap();
        assert!(full.trades.iter().all(|trade| trade.dividend_income == 0.0));

        let mut split_cfg = cfg();
        split_cfg.as_of = bars.rows[3].date;
        split_cfg.entry_expr = "close > 0".to_string();
        split_cfg.price_adjustment = PriceAdjustment::SplitsOnly;
        let split = run_backtest(split_cfg, &fetcher).unwrap();
        assert!(split.trades.iter().any(|trade| trade.dividend_income > 0.0));
    }

    #[test]
    fn rolling_backtest_enforces_reentry_cap() {
        let mut bars = make_flat_bars(30, 100.0);
        for idx in [5, 8, 11, 14] {
            bars.rows[idx].low = 70.0;
        }
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([
                ("AAA".to_string(), bars.clone()),
                ("SPY".to_string(), bars.clone()),
            ]),
        };
        let mut cfg = cfg();
        cfg.as_of = bars.rows[20].date;
        cfg.hold = 3;
        cfg.entry_expr = "close > 0".to_string();
        cfg.stop_loss = Some(0.05);
        cfg.allow_reentry = true;
        cfg.max_reentries = 1;
        let result =
            run_rolling_backtest(cfg, &fetcher, bars.rows[0].date, bars.rows[20].date).unwrap();
        assert_eq!(
            result.trades.iter().filter(|t| t.ticker == "AAA").count(),
            2
        );
    }

    #[test]
    fn time_exit_after_n_bars() {
        let bars = make_bars(20);
        let mut cfg = cfg();
        cfg.hold = 5;
        let trade = simulate_ticker(&bars, 3, &cfg, None)
            .unwrap()
            .trade
            .unwrap();
        assert_eq!(trade.exit_reason, ExitReason::Time);
        assert_eq!(trade.exit_date, bars.rows[9].date);
    }

    #[test]
    fn historical_and_rolling_match_for_single_signal_window() {
        let mut bars = make_bars(20);
        let spy = make_bars(20);
        set_extra(&mut bars, 5, "entry_signal", 1.0);
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([("AAA".to_string(), bars.clone()), ("SPY".to_string(), spy)]),
        };
        let mut cfg = cfg();
        cfg.as_of = bars.rows[5].date;
        cfg.hold = 3;
        cfg.entry_expr = "entry_signal > 0".to_string();

        let historical = run_backtest(cfg.clone(), &fetcher).unwrap();
        let rolling =
            run_rolling_backtest(cfg, &fetcher, bars.rows[5].date, bars.rows[9].date).unwrap();

        assert_eq!(historical.trades.len(), 1);
        assert_eq!(rolling.trades.len(), 1);
        let h_trade = &historical.trades[0];
        let r_trade = &rolling.trades[0];
        assert_eq!(h_trade.entry_date, bars.rows[6].date);
        assert_eq!(h_trade.entry_date, r_trade.entry_date);
        assert_eq!(h_trade.exit_date, r_trade.exit_date);
        assert_relative_eq!(h_trade.entry_price, r_trade.entry_price);
        assert_relative_eq!(h_trade.exit_price, r_trade.exit_price);
        assert_relative_eq!(
            historical.metrics["total_return"],
            rolling.metrics["total_return"]
        );
    }

    #[test]
    fn rolling_backtest_refills_freed_slot_from_same_day_signal() {
        let mut active = make_flat_bars(30, 100.0);
        let mut reserve = make_flat_bars(30, 50.0);
        let spy = make_flat_bars(30, 400.0);
        set_extra(&mut active, 5, "entry_signal", 1.0);
        set_extra(&mut reserve, 7, "entry_signal", 1.0);
        for row in &mut active.rows {
            row.volume = 1_000_000.0;
        }
        for row in &mut reserve.rows {
            row.volume = 500_000.0;
        }
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([
                ("ACTIVE".to_string(), active.clone()),
                ("RESERVE".to_string(), reserve.clone()),
                ("SPY".to_string(), spy),
            ]),
        };
        let mut cfg = cfg();
        cfg.as_of = active.rows[15].date;
        cfg.hold = 1;
        cfg.entry_expr = "entry_signal > 0".to_string();
        cfg.tickers = Some(vec!["ACTIVE".to_string(), "RESERVE".to_string()]);
        let result =
            run_rolling_backtest(cfg, &fetcher, active.rows[0].date, active.rows[15].date).unwrap();
        let active_trade = result
            .trades
            .iter()
            .find(|trade| trade.ticker == "ACTIVE")
            .unwrap();
        let reserve_trade = result
            .trades
            .iter()
            .find(|trade| trade.ticker == "RESERVE")
            .unwrap();
        assert_eq!(active_trade.exit_date, active.rows[7].date);
        assert_eq!(reserve_trade.signal_date, active_trade.exit_date);
        assert_eq!(reserve_trade.entry_date, reserve.rows[8].date);
    }

    #[test]
    fn rolling_backtest_force_closes_entry_on_window_end() {
        let mut bars = make_bars(12);
        let spy = make_bars(12);
        set_extra(&mut bars, 5, "entry_signal", 1.0);
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([("AAA".to_string(), bars.clone()), ("SPY".to_string(), spy)]),
        };
        let mut cfg = cfg();
        cfg.as_of = bars.rows[6].date;
        cfg.hold = 20;
        cfg.entry_expr = "entry_signal > 0".to_string();
        let result =
            run_rolling_backtest(cfg, &fetcher, bars.rows[0].date, bars.rows[6].date).unwrap();
        assert_eq!(result.trades.len(), 1);
        let trade = &result.trades[0];
        assert_eq!(trade.signal_date, bars.rows[5].date);
        assert_eq!(trade.entry_date, bars.rows[6].date);
        assert_eq!(trade.exit_date, bars.rows[6].date);
        assert_eq!(trade.exit_reason, ExitReason::Eod);
        assert_eq!(result.equity_curve.last().unwrap().0, bars.rows[6].date);
    }

    #[test]
    fn run_backtest_rs_breakout_us_selects_relative_strength_breakout_signal() {
        let mut aaa = trend_bars(80, 100.0, 150.0, 100_000.0);
        let mut bbb = trend_bars(80, 100.0, 108.0, 100_000.0);
        let spy = trend_bars(80, 100.0, 110.0, 100_000.0);
        aaa.rows[69].volume = 250_000.0;
        aaa.rows[70].open = 151.0;
        set_extra(&mut aaa, 69, "rs_breakout_entry", 1.0);
        set_extra(&mut bbb, 69, "rs_breakout_entry", 0.0);
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([
                ("AAA".to_string(), aaa.clone()),
                ("BBB".to_string(), bbb),
                ("SPY".to_string(), spy),
            ]),
        };
        let mut cfg = cfg();
        cfg.as_of = aaa.rows[69].date;
        cfg.hold = 3;
        cfg.entry_expr = "rs_breakout_entry > 0".to_string();
        cfg.strategy_name = Some("rs_breakout".to_string());
        cfg.tickers = Some(vec!["AAA".to_string(), "BBB".to_string()]);
        let result = run_backtest(cfg, &fetcher).unwrap();
        assert_eq!(result.selection[0].ticker, "AAA");
        assert_eq!(result.trades[0].ticker, "AAA");
        assert_eq!(result.trades[0].entry_date, aaa.rows[70].date);
    }

    #[test]
    fn run_backtest_rs_breakout_india_filters_to_delivery_qualified_signal() {
        let mut aaa = trend_bars(80, 100.0, 150.0, 100_000.0);
        let mut bbb = trend_bars(80, 100.0, 149.0, 100_000.0);
        let nifty = trend_bars(80, 100.0, 110.0, 100_000.0);
        set_extra(&mut aaa, 69, "rs_breakout_entry", 1.0);
        set_extra(&mut bbb, 69, "rs_breakout_entry", 0.0);
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([
                ("AAA.NS".to_string(), aaa.clone()),
                ("BBB.NS".to_string(), bbb),
                ("^NSEI".to_string(), nifty),
            ]),
        };
        let mut cfg = cfg();
        cfg.market = "india".to_string();
        cfg.benchmark = "^NSEI".to_string();
        cfg.as_of = aaa.rows[69].date;
        cfg.hold = 3;
        cfg.top = 2;
        cfg.entry_expr = "rs_breakout_entry > 0".to_string();
        cfg.strategy_name = Some("rs_breakout".to_string());
        cfg.tickers = Some(vec!["AAA".to_string(), "BBB".to_string()]);
        let result = run_backtest(cfg, &fetcher).unwrap();
        assert_eq!(result.selection[0].ticker, "AAA");
        assert_eq!(
            result
                .trades
                .iter()
                .map(|trade| trade.ticker.as_str())
                .collect::<Vec<_>>(),
            vec!["AAA"]
        );
    }

    #[test]
    fn rolling_rs_breakout_us_smoke() {
        let mut aaa = trend_bars(80, 100.0, 150.0, 100_000.0);
        let bbb = trend_bars(80, 100.0, 108.0, 100_000.0);
        let spy = trend_bars(80, 100.0, 110.0, 100_000.0);
        set_extra(&mut aaa, 69, "rs_breakout_entry", 1.0);
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([
                ("AAA".to_string(), aaa.clone()),
                ("BBB".to_string(), bbb),
                ("SPY".to_string(), spy),
            ]),
        };
        let mut cfg = cfg();
        cfg.as_of = aaa.rows[75].date;
        cfg.hold = 3;
        cfg.entry_expr = "rs_breakout_entry > 0".to_string();
        cfg.strategy_name = Some("rs_breakout".to_string());
        cfg.tickers = Some(vec!["AAA".to_string(), "BBB".to_string()]);
        let result =
            run_rolling_backtest(cfg, &fetcher, aaa.rows[65].date, aaa.rows[75].date).unwrap();
        assert!(!result.trades.is_empty());
        assert_eq!(result.trades[0].ticker, "AAA");
    }

    #[test]
    fn rolling_rs_breakout_india_delivery_filter() {
        let mut aaa = trend_bars(80, 100.0, 150.0, 100_000.0);
        let mut bbb = trend_bars(80, 100.0, 149.0, 100_000.0);
        let nifty = trend_bars(80, 100.0, 110.0, 100_000.0);
        set_extra(&mut aaa, 69, "rs_breakout_entry", 1.0);
        set_extra(&mut bbb, 69, "rs_breakout_entry", 0.0);
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([
                ("AAA.NS".to_string(), aaa.clone()),
                ("BBB.NS".to_string(), bbb),
                ("^NSEI".to_string(), nifty),
            ]),
        };
        let mut cfg = cfg();
        cfg.market = "india".to_string();
        cfg.benchmark = "^NSEI".to_string();
        cfg.as_of = aaa.rows[75].date;
        cfg.hold = 3;
        cfg.top = 2;
        cfg.entry_expr = "rs_breakout_entry > 0".to_string();
        cfg.strategy_name = Some("rs_breakout".to_string());
        cfg.tickers = Some(vec!["AAA".to_string(), "BBB".to_string()]);
        let result =
            run_rolling_backtest(cfg, &fetcher, aaa.rows[65].date, aaa.rows[75].date).unwrap();
        assert_eq!(
            result
                .trades
                .iter()
                .map(|trade| trade.ticker.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["AAA"])
        );
    }

    #[test]
    fn cash_stays_cash_after_exit_two_ticker_portfolio_all_days() {
        let mut bars_a = make_bars(20);
        let mut bars_b = make_bars(20);
        bars_a.rows[4].open = 100.0;
        bars_a.rows[6].close = 110.0;
        bars_b.rows[4].open = 50.0;
        let trade_a = simulate_ticker(
            &bars_a,
            3,
            &{
                let mut c = cfg();
                c.hold = 2;
                c
            },
            None,
        )
        .unwrap()
        .trade
        .unwrap();
        let trade_b = simulate_ticker(
            &bars_b,
            3,
            &{
                let mut c = cfg();
                c.hold = 15;
                c
            },
            None,
        )
        .unwrap()
        .trade
        .unwrap();
        let mut p_a = Portfolio::new(100_000.0, 2).unwrap();
        p_a.assign("AAA", 1, bars_a.rows[3].date);
        p_a.open("AAA", trade_a.entry_date, trade_a.entry_price, 0.0, true)
            .unwrap();
        let trade_a = p_a
            .close(
                "AAA",
                trade_a.exit_date,
                trade_a.exit_price,
                trade_a.exit_reason,
                0.0,
            )
            .unwrap();
        let mut p_b = Portfolio::new(100_000.0, 2).unwrap();
        p_b.assign("BBB", 2, bars_b.rows[3].date);
        p_b.open("BBB", trade_b.entry_date, trade_b.entry_price, 0.0, true)
            .unwrap();
        let trade_b = p_b
            .close(
                "BBB",
                trade_b.exit_date,
                trade_b.exit_price,
                trade_b.exit_reason,
                0.0,
            )
            .unwrap();
        let panel = BTreeMap::from([
            ("AAA".to_string(), bars_a.clone()),
            ("BBB".to_string(), bars_b.clone()),
        ]);
        let equity = build_equity_curve(
            &bars_a.dates(),
            &[trade_a.clone(), trade_b.clone()],
            &panel,
            100_000.0,
        );
        let static_cash = 100_000.0 - trade_a.entry_cost - trade_b.entry_cost + trade_a.exit_value;
        let final_cash = 100_000.0 - trade_a.entry_cost + trade_a.exit_value - trade_b.entry_cost
            + trade_b.exit_value;
        for (day, value) in equity.iter().filter(|(day, _)| *day > trade_a.exit_date) {
            let expected = if *day > trade_b.exit_date {
                final_cash
            } else {
                let b_close = bars_b
                    .rows
                    .iter()
                    .find(|bar| bar.date == *day)
                    .unwrap()
                    .close;
                static_cash + trade_b.shares * b_close
            };
            assert_relative_eq!(*value, expected);
        }
    }

    #[test]
    fn output_rank_preserves_selection_rank_not_realized_return() {
        let mut aaa = make_bars(60);
        let mut bbb = make_bars(60);
        let mut ccc = make_bars(60);
        for row in &mut aaa.rows {
            row.volume = 1_000_000.0;
        }
        for row in &mut bbb.rows {
            row.volume = 500_000.0;
        }
        for row in &mut ccc.rows {
            row.volume = 100_000.0;
        }
        for bars in [&mut aaa, &mut bbb, &mut ccc] {
            bars.rows[39].close += 20.0;
        }
        for i in 40..60 {
            set_bar(&mut aaa, i, 50.0, 51.0, 49.0, 50.0);
            set_bar(&mut ccc, i, 40.0, 41.0, 39.0, 40.0);
        }
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([
                ("AAA".to_string(), aaa.clone()),
                ("BBB".to_string(), bbb.clone()),
                ("CCC".to_string(), ccc),
                ("SPY".to_string(), bbb),
            ]),
        };
        let mut cfg = cfg();
        cfg.as_of = aaa.rows[39].date;
        cfg.hold = 10;
        cfg.top = 3;
        cfg.tickers = Some(vec![
            "AAA".to_string(),
            "BBB".to_string(),
            "CCC".to_string(),
        ]);
        let result = run_backtest(cfg, &fetcher).unwrap();
        let mut trades = result.trades.clone();
        trades.sort_by_key(|trade| trade.rank);
        assert_eq!(
            trades
                .iter()
                .map(|trade| (trade.rank, trade.ticker.as_str()))
                .collect::<Vec<_>>(),
            vec![(1, "AAA"), (2, "BBB"), (3, "CCC")]
        );
    }

    #[test]
    fn insufficient_lookback_emits_warning() {
        let bars = make_bars(30);
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([
                ("AAA".to_string(), bars.clone()),
                ("SPY".to_string(), bars.clone()),
            ]),
        };
        let mut cfg = cfg();
        cfg.as_of = bars.rows.last().unwrap().date;
        cfg.entry_expr = "close > sma(close, 200)".to_string();
        let result = run_backtest(cfg, &fetcher).unwrap();
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("insufficient lookback"))
        );
    }

    #[test]
    fn run_backtest_errors_when_no_universe_provided() {
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::new(),
        };
        let mut cfg = cfg();
        cfg.tickers = None;
        cfg.universe_file = None;
        let err = run_backtest(cfg, &fetcher).unwrap_err();
        assert!(err.to_string().contains("No universe provided"));
    }

    #[test]
    fn min_price_filter_excludes_penny_stocks() {
        let mut penny = make_bars(60);
        let mut real = make_bars(60);
        for i in 37..40 {
            penny.rows[i].close = 0.30;
        }
        penny.rows[39].close = 0.80;
        real.rows[39].close += 5.0;
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([
                ("PENNY".to_string(), penny),
                ("REAL".to_string(), real.clone()),
                ("SPY".to_string(), real),
            ]),
        };
        let mut cfg = cfg();
        cfg.as_of = real.rows[39].date;
        cfg.hold = 3;
        cfg.top = 5;
        cfg.tickers = Some(vec!["PENNY".to_string(), "REAL".to_string()]);
        cfg.min_price = Some(1.0);
        let result = run_backtest(cfg, &fetcher).unwrap();
        let traded = result
            .trades
            .iter()
            .map(|trade| trade.ticker.as_str())
            .collect::<BTreeSet<_>>();
        assert!(!traded.contains("PENNY"));
        assert!(traded.contains("REAL"));
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("filtered") && warning.contains("price/liquidity"))
        );
    }

    #[test]
    fn min_avg_dollar_volume_filter_excludes_illiquid() {
        let mut liquid = make_bars(60);
        let mut illiquid = make_bars(60);
        for row in &mut liquid.rows {
            row.volume = 50_000_000.0;
        }
        for row in &mut illiquid.rows {
            row.volume = 1.0;
        }
        liquid.rows[39].close += 5.0;
        illiquid.rows[39].close += 5.0;
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([
                ("LIQ".to_string(), liquid.clone()),
                ("ILLIQ".to_string(), illiquid),
                ("SPY".to_string(), liquid),
            ]),
        };
        let mut cfg = cfg();
        cfg.as_of = liquid.rows[39].date;
        cfg.hold = 3;
        cfg.top = 5;
        cfg.tickers = Some(vec!["LIQ".to_string(), "ILLIQ".to_string()]);
        cfg.min_avg_dollar_volume = Some(1_000_000.0);
        let result = run_backtest(cfg, &fetcher).unwrap();
        let traded = result
            .trades
            .iter()
            .map(|trade| trade.ticker.as_str())
            .collect::<BTreeSet<_>>();
        assert!(traded.contains("LIQ"));
        assert!(!traded.contains("ILLIQ"));
    }

    #[test]
    fn no_reinvest_matches_legacy_leaves_cash_idle() {
        let mut active = make_bars(60);
        let mut reserve = make_flat_bars(60, 100.0);
        active.rows[39].close += 5.0;
        active.rows[40].open = 100.0;
        active.rows[41].low = 90.0;
        active.rows[41].close = 91.0;
        reserve.rows[39].close = 105.0;
        reserve.rows[41].close = 105.0;
        for row in &mut active.rows {
            row.volume = 1_000_000.0;
        }
        for row in &mut reserve.rows {
            row.volume = 100_000.0;
        }
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([
                ("ACTIVE".to_string(), active.clone()),
                ("RESERVE".to_string(), reserve.clone()),
                ("SPY".to_string(), reserve),
            ]),
        };
        let mut cfg = cfg();
        cfg.as_of = active.rows[39].date;
        cfg.hold = 10;
        cfg.tickers = Some(vec!["ACTIVE".to_string(), "RESERVE".to_string()]);
        cfg.stop_loss = Some(0.05);
        cfg.reinvest = false;
        let result = run_backtest(cfg, &fetcher).unwrap();
        let traded = result
            .trades
            .iter()
            .map(|trade| trade.ticker.as_str())
            .collect::<BTreeSet<_>>();
        assert!(traded.contains("ACTIVE"));
        assert!(!traded.contains("RESERVE"));
    }

    #[test]
    fn reserve_filter_rechecked_on_exit_day() {
        let mut active = make_bars(60);
        let mut crash = make_flat_bars(60, 5.0);
        let mut backup = make_flat_bars(60, 100.0);
        active.rows[39].close += 5.0;
        crash.rows[39].close = 8.0;
        backup.rows[39].close = 105.0;
        active.rows[40].open = 100.0;
        active.rows[41].low = 90.0;
        active.rows[41].close = 91.0;
        for i in 40..45 {
            set_bar(&mut crash, i, 0.5, 0.6, 0.4, 0.5);
        }
        crash.rows[41].close = 0.8;
        backup.rows[41].close = 105.0;
        for row in &mut active.rows {
            row.volume = 1_000_000.0;
        }
        for row in &mut crash.rows {
            row.volume = 500_000.0;
        }
        for row in &mut backup.rows {
            row.volume = 100_000.0;
        }
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([
                ("ACTIVE".to_string(), active.clone()),
                ("CRASH".to_string(), crash),
                ("BACKUP".to_string(), backup.clone()),
                ("SPY".to_string(), backup),
            ]),
        };
        let mut cfg = cfg();
        cfg.as_of = active.rows[39].date;
        cfg.hold = 10;
        cfg.tickers = Some(vec![
            "ACTIVE".to_string(),
            "CRASH".to_string(),
            "BACKUP".to_string(),
        ]);
        cfg.stop_loss = Some(0.05);
        cfg.min_price = Some(1.0);
        let result = run_backtest(cfg, &fetcher).unwrap();
        let traded = result
            .trades
            .iter()
            .map(|trade| trade.ticker.as_str())
            .collect::<BTreeSet<_>>();
        assert!(traded.contains("ACTIVE"));
        assert!(!traded.contains("CRASH"));
        assert!(traded.contains("BACKUP"));
    }

    #[test]
    fn invested_return_metric_ignores_idle_cash() {
        let trade = Trade {
            ticker: "X".to_string(),
            rank: 1,
            signal_date: d(2),
            entry_date: d(3),
            entry_price: 100.0,
            exit_date: d(5),
            exit_price: 110.0,
            exit_reason: ExitReason::Time,
            shares: 100.0,
            entry_cost: 10_000.0,
            exit_value: 11_000.0,
            pnl: 1_000.0,
            return_pct: 0.10,
            dividend_income: 0.0,
        };
        let equity = vec![(d(3), 100_000.0), (d(4), 100_500.0), (d(5), 101_000.0)];
        let bench = vec![(d(3), 100.0), (d(4), 100.0), (d(5), 100.0)];
        let metrics = compute_metrics(&equity, &bench, &[trade], 10, 1);
        assert_relative_eq!(metrics["invested_return"], 0.10, epsilon = 1e-6);
        assert_relative_eq!(metrics["total_return"], 0.01, epsilon = 1e-6);
    }

    #[test]
    fn metrics_on_known_ramp_series() {
        let start = d(1);
        let equity = (0..252)
            .map(|i| {
                (
                    start + Duration::days(i as i64),
                    100_000.0 + 10_000.0 * i as f64 / 251.0,
                )
            })
            .collect::<Vec<_>>();
        let metrics = compute_metrics(&equity, &equity, &[], 1, 1);
        assert_relative_eq!(metrics["total_return"], 0.10, epsilon = 1e-6);
        assert_relative_eq!(metrics["cagr"], 0.10, epsilon = 0.01);
        assert_relative_eq!(metrics["max_drawdown"], 0.0, epsilon = 1e-9);
        assert_relative_eq!(metrics["beta"], 1.0, epsilon = 1e-6);
    }

    #[test]
    fn rolling_backtest_honors_max_concurrent_per_ticker() {
        let mut bars = make_flat_bars(12, 100.0);
        for idx in 0..8 {
            set_extra(&mut bars, idx, "entry_signal", 1.0);
        }
        let spy = make_flat_bars(12, 400.0);
        let fetcher = StubPriceFetcher {
            panel: BTreeMap::from([("AAA".to_string(), bars.clone()), ("SPY".to_string(), spy)]),
        };
        let mut cfg = cfg();
        cfg.top = 2;
        cfg.hold = 5;
        cfg.entry_expr = "entry_signal > 0".to_string();
        cfg.allow_reentry = true;
        cfg.max_reentries = 10;
        cfg.max_concurrent_per_ticker = 2;
        let result =
            run_rolling_backtest(cfg, &fetcher, bars.rows[0].date, bars.rows[8].date).unwrap();
        let aaa = result
            .trades
            .iter()
            .filter(|trade| trade.ticker == "AAA")
            .collect::<Vec<_>>();
        assert!(aaa.len() >= 2);
        for day in bars.rows.iter().map(|bar| bar.date) {
            let open_count = aaa
                .iter()
                .filter(|trade| day >= trade.entry_date && day <= trade.exit_date)
                .count();
            assert!(open_count <= 2);
        }
    }
}
