use crate::backtester::models::Trade;
use chrono::NaiveDate;

pub fn maximum_drawdown(equity: &[(NaiveDate, f64)]) -> f64 {
    if equity.is_empty() {
        return 0.0;
    }
    let mut peak = equity[0].1;
    let mut worst = 0.0;
    for (_, value) in equity {
        if *value > peak {
            peak = *value;
        }
        if peak != 0.0 {
            let dd = (*value - peak) / peak;
            if dd < worst {
                worst = dd;
            }
        }
    }
    worst
}

pub fn profit_factor(trades: &[Trade]) -> f64 {
    let gross_profit: f64 = trades
        .iter()
        .filter(|trade| trade.pnl > 0.0)
        .map(|trade| trade.pnl)
        .sum();
    let gross_loss: f64 = trades
        .iter()
        .filter(|trade| trade.pnl < 0.0)
        .map(|trade| trade.pnl.abs())
        .sum();
    if gross_loss == 0.0 {
        if gross_profit > 0.0 {
            f64::INFINITY
        } else {
            0.0
        }
    } else {
        gross_profit / gross_loss
    }
}

pub fn win_rate(trades: &[Trade]) -> f64 {
    if trades.is_empty() {
        return 0.0;
    }
    trades.iter().filter(|trade| trade.pnl > 0.0).count() as f64 / trades.len() as f64
}

pub fn expectancy(trades: &[Trade]) -> f64 {
    if trades.is_empty() {
        return 0.0;
    }
    trades.iter().map(|trade| trade.return_pct).sum::<f64>() / trades.len() as f64
}

pub fn sharpe_ratio(equity: &[(NaiveDate, f64)]) -> f64 {
    if equity.len() < 2 {
        return 0.0;
    }
    let returns = equity
        .windows(2)
        .filter_map(|pair| {
            if pair[0].1 != 0.0 {
                Some(pair[1].1 / pair[0].1 - 1.0)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if returns.is_empty() {
        return 0.0;
    }
    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let std =
        (returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64).sqrt();
    if std == 0.0 {
        0.0
    } else {
        mean / std * 252.0_f64.sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backtester::models::ExitReason;
    use approx::assert_relative_eq;

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
            exit_price: 100.0 * (1.0 + return_pct),
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
    fn calculations_match_python_edge_cases() {
        let equity = vec![(d(1), 100.0), (d(2), 110.0), (d(3), 105.0), (d(4), 120.0)];
        let trades = vec![trade(10.0, 0.10), trade(-5.0, -0.05)];
        assert_relative_eq!(maximum_drawdown(&equity), -5.0 / 110.0);
        assert_relative_eq!(profit_factor(&trades), 2.0);
        assert_relative_eq!(win_rate(&trades), 0.5);
        assert_relative_eq!(expectancy(&trades), 0.025);
        assert_eq!(profit_factor(&[]), 0.0);
    }
}
