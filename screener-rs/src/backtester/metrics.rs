use crate::backtester::models::Trade;
use chrono::NaiveDate;
use std::collections::BTreeMap;

const TRADING_DAYS_PER_YEAR: f64 = 252.0;
const EULER_MASCHERONI: f64 = 0.577_215_664_901_532_9;

fn daily_returns(curve: &[(NaiveDate, f64)]) -> Vec<f64> {
    curve
        .windows(2)
        .filter_map(|pair| {
            let prev = pair[0].1;
            let curr = pair[1].1;
            if prev.is_finite() && curr.is_finite() && prev != 0.0 {
                Some(curr / prev - 1.0)
            } else {
                None
            }
        })
        .collect()
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn std_pop(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let avg = mean(values);
    let var = values.iter().map(|v| (v - avg).powi(2)).sum::<f64>() / values.len() as f64;
    var.sqrt()
}

fn cagr(curve: &[(NaiveDate, f64)]) -> f64 {
    if curve.len() < 2 || curve[0].1 <= 0.0 {
        return 0.0;
    }
    let years = (curve.len() as f64 / TRADING_DAYS_PER_YEAR).max(1e-9);
    (curve.last().unwrap().1 / curve[0].1).powf(1.0 / years) - 1.0
}

fn max_drawdown(curve: &[(NaiveDate, f64)]) -> f64 {
    if curve.is_empty() {
        return 0.0;
    }
    let mut peak = curve[0].1;
    let mut worst = 0.0;
    for (_, equity) in curve {
        if *equity > peak {
            peak = *equity;
        }
        if peak != 0.0 {
            let dd = (*equity - peak) / peak;
            if dd < worst {
                worst = dd;
            }
        }
    }
    worst
}

fn sharpe(daily: &[f64], rf: f64) -> f64 {
    let sd = std_pop(daily);
    if daily.is_empty() || sd == 0.0 {
        return 0.0;
    }
    let excess: Vec<f64> = daily
        .iter()
        .map(|r| r - rf / TRADING_DAYS_PER_YEAR)
        .collect();
    mean(&excess) / std_pop(&excess) * TRADING_DAYS_PER_YEAR.sqrt()
}

fn vol_annual(daily: &[f64]) -> f64 {
    std_pop(daily) * TRADING_DAYS_PER_YEAR.sqrt()
}

fn sortino(daily: &[f64], rf: f64) -> f64 {
    if daily.is_empty() {
        return 0.0;
    }
    let excess: Vec<f64> = daily
        .iter()
        .map(|r| r - rf / TRADING_DAYS_PER_YEAR)
        .collect();
    let downside: Vec<f64> = excess.iter().copied().filter(|r| *r < 0.0).collect();
    let downside_std = std_pop(&downside);
    if downside.is_empty() || downside_std == 0.0 {
        return 0.0;
    }
    mean(&excess) / downside_std * TRADING_DAYS_PER_YEAR.sqrt()
}

fn calmar(curve: &[(NaiveDate, f64)]) -> f64 {
    let mdd = max_drawdown(curve);
    if curve.len() < 2 || mdd >= 0.0 {
        return 0.0;
    }
    cagr(curve) / mdd.abs()
}

fn alpha_beta(equity_daily: &[f64], benchmark_daily: &[f64]) -> (f64, f64) {
    let n = equity_daily.len().min(benchmark_daily.len());
    if n < 2 {
        return (0.0, 0.0);
    }
    let x = &benchmark_daily[..n];
    let y = &equity_daily[..n];
    let mx = mean(x);
    let my = mean(y);
    let var_x = x.iter().map(|v| (v - mx).powi(2)).sum::<f64>() / n as f64;
    if var_x == 0.0 {
        return (0.0, 0.0);
    }
    let cov = x
        .iter()
        .zip(y)
        .map(|(a, b)| (a - mx) * (b - my))
        .sum::<f64>()
        / n as f64;
    let beta = cov / var_x;
    let intercept = my - beta * mx;
    (intercept * TRADING_DAYS_PER_YEAR, beta)
}

fn exposure(equity: &[(NaiveDate, f64)], trades: &[Trade], slot_count: usize) -> f64 {
    if trades.is_empty() || equity.is_empty() {
        return 0.0;
    }
    let total_open: usize = equity
        .iter()
        .map(|(day, _)| {
            trades
                .iter()
                .filter(|trade| *day >= trade.entry_date && *day <= trade.exit_date)
                .count()
        })
        .sum();
    total_open as f64 / equity.len() as f64 / slot_count.max(1) as f64
}

fn phi(x: f64) -> f64 {
    0.5 * (1.0 + erf(x / 2.0_f64.sqrt()))
}

fn erf(x: f64) -> f64 {
    // Abramowitz and Stegun 7.1.26; sufficient for PSR/DSR gating metrics.
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-x * x).exp();
    sign * y
}

fn phi_inv(p: f64) -> f64 {
    if p <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    let (mut lo, mut hi) = (-8.0, 8.0);
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if phi(mid) < p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

fn skew(values: &[f64]) -> f64 {
    let sd = std_pop(values);
    if values.is_empty() || sd == 0.0 {
        return 0.0;
    }
    let avg = mean(values);
    values.iter().map(|v| ((v - avg) / sd).powi(3)).sum::<f64>() / values.len() as f64
}

fn kurt_excess(values: &[f64]) -> f64 {
    let sd = std_pop(values);
    if values.is_empty() || sd == 0.0 {
        return 0.0;
    }
    let avg = mean(values);
    values.iter().map(|v| ((v - avg) / sd).powi(4)).sum::<f64>() / values.len() as f64 - 3.0
}

fn psr(daily: &[f64], sr_benchmark_annual: f64) -> f64 {
    if daily.len() < 30 {
        return 0.0;
    }
    let sr_per = sharpe(daily, 0.0) / TRADING_DAYS_PER_YEAR.sqrt();
    let sr_bench_per = sr_benchmark_annual / TRADING_DAYS_PER_YEAR.sqrt();
    let denom_sq = 1.0 - skew(daily) * sr_per + (kurt_excess(daily) / 4.0) * sr_per.powi(2);
    let denom = denom_sq.max(1e-12).sqrt();
    phi((sr_per - sr_bench_per) * ((daily.len() - 1) as f64).sqrt() / denom)
}

fn dsr(daily: &[f64], n_trials: usize, sr_trial_std_annual: f64) -> f64 {
    if n_trials <= 1 {
        return psr(daily, 0.0);
    }
    let n = n_trials as f64;
    let sr0_annual = sr_trial_std_annual
        * ((1.0 - EULER_MASCHERONI) * phi_inv(1.0 - 1.0 / n)
            + EULER_MASCHERONI * phi_inv(1.0 - 1.0 / (n * std::f64::consts::E)));
    psr(daily, sr0_annual)
}

fn invested_return(trades: &[Trade]) -> f64 {
    let total_cost: f64 = trades.iter().map(|trade| trade.entry_cost).sum();
    let total_pnl: f64 = trades.iter().map(|trade| trade.pnl).sum();
    if total_cost <= 0.0 {
        0.0
    } else {
        total_pnl / total_cost
    }
}

pub fn compute_metrics(
    equity: &[(NaiveDate, f64)],
    benchmark: &[(NaiveDate, f64)],
    trades: &[Trade],
    slot_count: usize,
    n_trials: usize,
) -> BTreeMap<String, f64> {
    let daily = daily_returns(equity);
    let bench_daily = daily_returns(benchmark);
    let total_return = if equity.len() >= 2 && equity[0].1 > 0.0 {
        equity.last().unwrap().1 / equity[0].1 - 1.0
    } else {
        0.0
    };
    let benchmark_return = if benchmark.len() >= 2 && benchmark[0].1 > 0.0 {
        benchmark.last().unwrap().1 / benchmark[0].1 - 1.0
    } else {
        0.0
    };
    let (alpha, beta) = alpha_beta(&daily, &bench_daily);
    let hit_rate = if trades.is_empty() {
        0.0
    } else {
        trades.iter().filter(|trade| trade.pnl > 0.0).count() as f64 / trades.len() as f64
    };
    BTreeMap::from([
        ("total_return".to_string(), total_return),
        ("cagr".to_string(), cagr(equity)),
        ("vol_annual".to_string(), vol_annual(&daily)),
        ("sharpe".to_string(), sharpe(&daily, 0.0)),
        ("sortino".to_string(), sortino(&daily, 0.0)),
        ("calmar".to_string(), calmar(equity)),
        ("psr".to_string(), psr(&daily, 0.0)),
        ("dsr".to_string(), dsr(&daily, n_trials, 0.5)),
        ("max_drawdown".to_string(), max_drawdown(equity)),
        ("hit_rate".to_string(), hit_rate),
        ("alpha_annual".to_string(), alpha),
        ("beta".to_string(), beta),
        ("exposure".to_string(), exposure(equity, trades, slot_count)),
        ("benchmark_return".to_string(), benchmark_return),
        ("trade_count".to_string(), trades.len() as f64),
        ("invested_return".to_string(), invested_return(trades)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, day).unwrap()
    }

    #[test]
    fn max_drawdown_metric_matches_python_case() {
        let eq = vec![(d(1), 100.0), (d(2), 110.0), (d(3), 105.0), (d(4), 120.0)];
        let metrics = compute_metrics(&eq, &eq, &[], 1, 1);
        assert_relative_eq!(metrics["max_drawdown"], -5.0 / 110.0);
    }
}
