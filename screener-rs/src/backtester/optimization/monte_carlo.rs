use crate::backtester::models::Trade;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MonteCarloResult {
    pub iterations: usize,
    pub return_p05: f64,
    pub return_p50: f64,
    pub return_p95: f64,
    pub max_drawdown_p95: f64,
    pub ruin_probability: f64,
}

#[derive(Debug, Clone)]
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }

    fn gen_range(&mut self, upper: usize) -> usize {
        (self.next_u64() as usize) % upper
    }
}

pub fn simulate_monte_carlo(
    trades: &[Trade],
    iterations: usize,
    seed: u64,
    initial_capital: f64,
    ruin_threshold: f64,
) -> MonteCarloResult {
    if trades.is_empty() || iterations == 0 {
        return MonteCarloResult {
            iterations,
            return_p05: 0.0,
            return_p50: 0.0,
            return_p95: 0.0,
            max_drawdown_p95: 0.0,
            ruin_probability: 0.0,
        };
    }
    let returns = trades
        .iter()
        .map(|trade| trade.return_pct)
        .collect::<Vec<_>>();
    let mut rng = Lcg::new(seed);
    let mut terminal_returns = Vec::with_capacity(iterations);
    let mut drawdowns = Vec::with_capacity(iterations);
    let mut ruin_count = 0_usize;

    for _ in 0..iterations {
        let mut equity = initial_capital;
        let mut peak = initial_capital;
        let mut worst_dd = 0.0;
        for _ in 0..returns.len() {
            let ret = returns[rng.gen_range(returns.len())];
            equity *= 1.0 + ret;
            if equity > peak {
                peak = equity;
            }
            if peak > 0.0 {
                let dd = (equity - peak) / peak;
                if dd < worst_dd {
                    worst_dd = dd;
                }
            }
        }
        if equity <= initial_capital * ruin_threshold {
            ruin_count += 1;
        }
        terminal_returns.push(equity / initial_capital - 1.0);
        drawdowns.push(worst_dd);
    }

    terminal_returns.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    drawdowns.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    MonteCarloResult {
        iterations,
        return_p05: percentile(&terminal_returns, 0.05),
        return_p50: percentile(&terminal_returns, 0.50),
        return_p95: percentile(&terminal_returns, 0.95),
        max_drawdown_p95: percentile(&drawdowns, 0.05),
        ruin_probability: ruin_count as f64 / iterations as f64,
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backtester::models::{ExitReason, Trade};
    use chrono::NaiveDate;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, day).unwrap()
    }

    fn trade(pnl: f64, return_pct: f64) -> Trade {
        Trade {
            ticker: "AAA".to_string(),
            rank: 1,
            signal_date: d(1),
            entry_date: d(2),
            entry_price: 100.0,
            exit_date: d(3),
            exit_price: 100.0,
            exit_reason: ExitReason::Time,
            shares: 1.0,
            entry_cost: 100.0,
            exit_value: 100.0 + pnl,
            pnl,
            return_pct,
            dividend_income: 0.0,
        }
    }

    #[test]
    fn monte_carlo_is_reproducible() {
        let trades = vec![trade(10.0, 0.10), trade(-5.0, -0.05), trade(3.0, 0.03)];
        let a = simulate_monte_carlo(&trades, 100, 7, 100_000.0, 0.5);
        let b = simulate_monte_carlo(&trades, 100, 7, 100_000.0, 0.5);
        assert_eq!(a, b);
    }
}
