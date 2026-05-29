use crate::backtester::models::{ExitReason, Position, Trade};
use crate::data::Bars;
use chrono::NaiveDate;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct Portfolio {
    initial_capital: f64,
    slot_count: usize,
    slot_capital: f64,
    cash: f64,
    open: BTreeMap<(String, usize), Position>,
    open_seq: BTreeMap<String, usize>,
    closed: Vec<Trade>,
    ranks: BTreeMap<String, usize>,
    signal_dates: BTreeMap<String, NaiveDate>,
}

impl Portfolio {
    pub fn new(initial_capital: f64, slot_count: usize) -> anyhow::Result<Self> {
        if slot_count == 0 {
            anyhow::bail!("slot_count must be > 0");
        }
        Ok(Self {
            initial_capital,
            slot_count,
            slot_capital: initial_capital / slot_count as f64,
            cash: initial_capital,
            open: BTreeMap::new(),
            open_seq: BTreeMap::new(),
            closed: Vec::new(),
            ranks: BTreeMap::new(),
            signal_dates: BTreeMap::new(),
        })
    }

    pub fn initial_capital(&self) -> f64 {
        self.initial_capital
    }

    pub fn slot_count(&self) -> usize {
        self.slot_count
    }

    pub fn assign(&mut self, ticker: &str, rank: usize, signal_date: NaiveDate) {
        self.ranks.insert(ticker.to_string(), rank);
        self.signal_dates.insert(ticker.to_string(), signal_date);
    }

    fn active_keys(&self, ticker: &str) -> Vec<(String, usize)> {
        self.open
            .keys()
            .filter(|(sym, _)| sym == ticker)
            .cloned()
            .collect()
    }

    fn oldest_key(&self, ticker: &str) -> Option<(String, usize)> {
        self.active_keys(ticker)
            .into_iter()
            .min_by_key(|(_, seq)| *seq)
    }

    pub fn open(
        &mut self,
        ticker: &str,
        entry_date: NaiveDate,
        entry_price: f64,
        commission_bps: f64,
        raise_if_exists: bool,
    ) -> anyhow::Result<Position> {
        if raise_if_exists && !self.active_keys(ticker).is_empty() {
            anyhow::bail!("Position already open for {ticker}");
        }
        let c = commission_bps / 10_000.0;
        let gross_per_share = entry_price * (1.0 + c);
        let budget = self.slot_capital.min(self.cash.max(0.0));
        let shares = if gross_per_share > 0.0 {
            budget / gross_per_share
        } else {
            0.0
        };
        let notional = shares * entry_price;
        let commission = notional * c;
        let entry_cost = notional + commission;
        self.cash -= entry_cost;
        let position = Position {
            ticker: ticker.to_string(),
            entry_date,
            entry_fill: entry_price,
            shares,
            slot_capital: entry_cost,
            peak_price: entry_price,
            dividend_income: 0.0,
        };
        let seq = self.open_seq.get(ticker).copied().unwrap_or(0) + 1;
        self.open_seq.insert(ticker.to_string(), seq);
        self.open
            .insert((ticker.to_string(), seq), position.clone());
        Ok(position)
    }

    pub fn credit_dividends(&mut self, ticker: &str, cash_per_share: f64) -> f64 {
        if cash_per_share <= 0.0 {
            return 0.0;
        }
        let mut total = 0.0;
        for ((sym, _), pos) in self.open.iter_mut() {
            if sym != ticker || pos.shares <= 0.0 {
                continue;
            }
            let credit = pos.shares * cash_per_share;
            self.cash += credit;
            pos.dividend_income += credit;
            total += credit;
        }
        total
    }

    pub fn close(
        &mut self,
        ticker: &str,
        exit_date: NaiveDate,
        exit_price: f64,
        reason: ExitReason,
        commission_bps: f64,
    ) -> anyhow::Result<Trade> {
        let key = self
            .oldest_key(ticker)
            .ok_or_else(|| anyhow::anyhow!("No open position for {ticker}"))?;
        let position = self.open.remove(&key).expect("key exists");
        let c = commission_bps / 10_000.0;
        let proceeds = position.shares * exit_price;
        let commission = proceeds * c;
        let exit_value = proceeds - commission;
        self.cash += exit_value;
        let entry_cost = position.slot_capital;
        let pnl = exit_value - entry_cost;
        let return_pct = if entry_cost != 0.0 {
            pnl / entry_cost
        } else {
            0.0
        };
        let trade = Trade {
            ticker: ticker.to_string(),
            rank: self.ranks.get(ticker).copied().unwrap_or(0),
            signal_date: self
                .signal_dates
                .get(ticker)
                .copied()
                .unwrap_or(position.entry_date),
            entry_date: position.entry_date,
            entry_price: position.entry_fill,
            exit_date,
            exit_price,
            exit_reason: reason,
            shares: position.shares,
            entry_cost,
            exit_value,
            pnl,
            return_pct,
            dividend_income: position.dividend_income,
        };
        self.closed.push(trade.clone());
        Ok(trade)
    }

    pub fn partial_close(
        &mut self,
        ticker: &str,
        exit_date: NaiveDate,
        exit_price: f64,
        reason: ExitReason,
        fraction: f64,
        commission_bps: f64,
    ) -> anyhow::Result<Trade> {
        if !(0.0..=1.0).contains(&fraction) || fraction == 0.0 {
            anyhow::bail!("fraction must be in (0, 1]; got {fraction}");
        }
        if fraction >= 1.0 {
            return self.close(ticker, exit_date, exit_price, reason, commission_bps);
        }
        let key = self
            .oldest_key(ticker)
            .ok_or_else(|| anyhow::anyhow!("No open position for {ticker}"))?;
        let position = self.open.get_mut(&key).expect("key exists");
        let close_shares = position.shares * fraction;
        let pro_rata_cost = position.slot_capital * fraction;
        let pro_rata_div = position.dividend_income * fraction;
        let c = commission_bps / 10_000.0;
        let proceeds = close_shares * exit_price;
        let commission = proceeds * c;
        let exit_value = proceeds - commission;
        self.cash += exit_value;
        let pnl = exit_value - pro_rata_cost;
        let return_pct = if pro_rata_cost != 0.0 {
            pnl / pro_rata_cost
        } else {
            0.0
        };
        let trade = Trade {
            ticker: ticker.to_string(),
            rank: self.ranks.get(ticker).copied().unwrap_or(0),
            signal_date: self
                .signal_dates
                .get(ticker)
                .copied()
                .unwrap_or(position.entry_date),
            entry_date: position.entry_date,
            entry_price: position.entry_fill,
            exit_date,
            exit_price,
            exit_reason: reason,
            shares: close_shares,
            entry_cost: pro_rata_cost,
            exit_value,
            pnl,
            return_pct,
            dividend_income: pro_rata_div,
        };
        self.closed.push(trade.clone());
        position.shares -= close_shares;
        position.slot_capital -= pro_rata_cost;
        position.dividend_income -= pro_rata_div;
        Ok(trade)
    }

    pub fn get_position(&self, ticker: &str) -> Option<&Position> {
        self.oldest_key(ticker).and_then(|key| self.open.get(&key))
    }

    pub fn closed_trades(&self) -> Vec<Trade> {
        self.closed.clone()
    }

    pub fn open_tickers(&self) -> BTreeSet<String> {
        self.open.keys().map(|(ticker, _)| ticker.clone()).collect()
    }

    pub fn cash(&self) -> f64 {
        self.cash
    }
}

pub fn build_equity_curve(
    calendar: &[NaiveDate],
    trades: &[Trade],
    price_panel: &BTreeMap<String, Bars>,
    initial_capital: f64,
) -> Vec<(NaiveDate, f64)> {
    let mut events = Vec::new();
    for (seq, trade) in trades.iter().enumerate() {
        events.push((trade.entry_date, 1_u8, seq, trade));
        events.push((trade.exit_date, 0_u8, seq, trade));
    }
    events.sort_by_key(|(day, kind, seq, _)| (*day, *kind, *seq));

    let mut cash = initial_capital;
    let mut open_positions: BTreeMap<usize, &Trade> = BTreeMap::new();
    let mut ev_idx = 0;
    let mut out = Vec::with_capacity(calendar.len());

    for day in calendar {
        while ev_idx < events.len() && events[ev_idx].0 <= *day {
            let (_, kind, seq, trade) = events[ev_idx];
            if kind == 1 {
                cash -= trade.entry_cost;
                open_positions.insert(seq, trade);
            } else {
                open_positions.remove(&seq);
                cash += trade.exit_value;
            }
            ev_idx += 1;
        }
        let mut mtm = 0.0;
        for trade in open_positions.values() {
            let price = price_panel
                .get(&trade.ticker)
                .and_then(|bars| {
                    bars.position_on_or_before(*day)
                        .and_then(|idx| bars.get(idx))
                        .map(|bar| bar.close)
                })
                .filter(|px| px.is_finite())
                .unwrap_or(trade.entry_price);
            mtm += trade.shares * price;
        }
        out.push((*day, cash + mtm));
    }
    out
}
