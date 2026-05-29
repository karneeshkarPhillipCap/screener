use chrono::{Duration, NaiveDate};
use screener_rs::data::{Bar, Bars};
use screener_rs::indicators::{atr, ema, rsi, sma};
use std::collections::BTreeMap;

const CHECKPOINTS: [usize; 6] = [19, 20, 50, 199, 200, 299];
const SMA_20_REF: [f64; 6] = [
    102.855493082119665,
    102.967183948465092,
    99.031952218037432,
    109.332100368956574,
    109.278479921588385,
    112.746741420686618,
];
const EMA_20_REF: [f64; 6] = [
    102.904665321196802,
    102.926489321740803,
    99.368838072205193,
    108.770465557288944,
    108.702173929917123,
    112.860584559609123,
];
const RSI_14_REF: [f64; 6] = [
    86.162253586563551,
    80.442883076822454,
    47.116332997112110,
    44.283463524411829,
    40.583485004608690,
    93.472973867862365,
];
const ATR_14_REF: [f64; 6] = [
    2.264387192027122,
    2.269772245662709,
    2.283053065998752,
    2.305911992871662,
    2.300979657544947,
    2.411926067271472,
];

fn synthetic_bars() -> Bars {
    let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
    Bars::new(
        (0..300)
            .map(|i| {
                let x = i as f64;
                let close = 100.0 + 0.04 * x + 2.5 * (x / 9.0).sin() + 0.9 * (x / 17.0).cos();
                let open = close - 0.25 + 0.18 * (x / 6.0).sin();
                let high = open.max(close) + 1.0 + 0.12 * (x / 4.0).cos().powi(2);
                let low = open.min(close) - 1.0 - 0.08 * (x / 5.0).sin().powi(2);
                Bar {
                    date: start + Duration::days(i as i64),
                    open,
                    high,
                    low,
                    close,
                    volume: 100_000.0 + 1000.0 * ((i * 37) % 23) as f64,
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
fn indicators_match_python_reference_values_on_300_bar_series() {
    let bars = synthetic_bars();
    let close = bars.series("close").unwrap();
    let sma_values = sma(&close, 20);
    let ema_values = ema(&close, 20);
    let rsi_values = rsi(&close, 14);
    let atr_values = atr(&bars, 14);

    for (offset, idx) in CHECKPOINTS.iter().copied().enumerate() {
        assert_close(sma_values[idx], SMA_20_REF[offset], 1e-9);
        assert_close(ema_values[idx], EMA_20_REF[offset], 1e-9);
        assert_close(rsi_values[idx], RSI_14_REF[offset], 1e-3);
        assert_close(atr_values[idx], ATR_14_REF[offset], 1e-2);
    }
}
