#![allow(clippy::excessive_precision)]

use chrono::{Duration, NaiveDate};
use screener_rs::data::{Bar, Bars};
use screener_rs::indicators::{atr, ema, rsi, sma, supertrend_dir};
use screener_rs::screeners::rs_breakout::supertrend_values;
use std::collections::BTreeMap;

const CHECKPOINTS: [usize; 6] = [19, 20, 50, 199, 200, 259];
const SMA_20_REF: [f64; 6] = [
    102.301978099088018,
    102.346172559432375,
    100.730177446449588,
    110.499695400544212,
    110.422261046097816,
    111.844522977001546,
];
const EMA_20_REF: [f64; 6] = [
    102.196623330518904,
    102.138267699696826,
    101.448604503475281,
    109.953760763519554,
    109.839192052186746,
    111.825047882391175,
];
const RSI_14_REF: [f64; 3] = [33.307383095900803, 30.239527153845714, 53.844661603881718];
const ATR_14_REF: [f64; 3] = [2.351612492456674, 2.341999244570029, 2.376585022920336];
const SUPERTREND_REF: [f64; 6] = [
    96.075922804575953,
    96.075922804575953,
    96.201060685498476,
    103.596121002765102,
    103.596121002765102,
    106.935110275199847,
];

fn synthetic_bars(n: usize) -> Bars {
    let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
    Bars::new(
        (0..n)
            .map(|i| {
                let x = i as f64;
                let close = 100.0 + 0.05 * x + 2.0 * (x / 7.0).sin() + 0.7 * (x / 13.0).cos();
                let open = close - 0.3 + 0.2 * (x / 5.0).sin();
                let high = open.max(close) + 1.0 + 0.1 * (x / 3.0).cos().powi(2);
                let low = open.min(close) - 1.0 - 0.1 * (x / 4.0).sin().powi(2);
                Bar {
                    date: start + Duration::days(i as i64),
                    open,
                    high,
                    low,
                    close,
                    volume: 100_000.0,
                    adj_close: None,
                    dividend: None,
                    extra: BTreeMap::new(),
                }
            })
            .collect(),
    )
}

fn assert_close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() < tolerance,
        "actual={actual:.15}, expected={expected:.15}, tolerance={tolerance}"
    );
}

#[test]
fn sma_and_ema_match_python_reference_values() {
    let bars = synthetic_bars(260);
    let close = bars.series("close").unwrap();
    let sma_values = sma(&close, 20);
    let ema_values = ema(&close, 20);

    for (offset, idx) in CHECKPOINTS.iter().copied().enumerate() {
        assert_close(sma_values[idx], SMA_20_REF[offset], 1e-9);
        assert_close(ema_values[idx], EMA_20_REF[offset], 1e-9);
    }
}

#[test]
fn rsi_and_atr_match_python_reference_values_after_warmup() {
    let bars = synthetic_bars(260);
    let close = bars.series("close").unwrap();
    let rsi_values = rsi(&close, 14);
    let atr_values = atr(&bars, 14);

    for (offset, idx) in [199, 200, 259].iter().copied().enumerate() {
        assert_close(rsi_values[idx], RSI_14_REF[offset], 1e-3);
        assert_close(atr_values[idx], ATR_14_REF[offset], 1e-9);
    }
}

#[test]
fn supertrend_matches_python_rs_breakout_reference_values() {
    let bars = synthetic_bars(260);
    let supertrend = supertrend_values(&bars, 10, 3.0);
    let direction = supertrend_dir(&bars, 10, 3.0);

    for (offset, idx) in CHECKPOINTS.iter().copied().enumerate() {
        assert_close(supertrend[idx], SUPERTREND_REF[offset], 1e-9);
        let close = bars.rows[idx].close;
        let expected_direction = if close >= supertrend[idx] { -1.0 } else { 1.0 };
        assert_eq!(direction[idx], expected_direction);
    }
}
