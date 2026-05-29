use crate::data::Bars;

pub fn sma(source: &[f64], length: usize) -> Vec<f64> {
    rolling(source, length, |window| {
        window.iter().sum::<f64>() / length as f64
    })
}

pub fn highest(source: &[f64], length: usize) -> Vec<f64> {
    rolling(source, length, |window| {
        window.iter().copied().fold(f64::NEG_INFINITY, f64::max)
    })
}

pub fn lowest(source: &[f64], length: usize) -> Vec<f64> {
    rolling(source, length, |window| {
        window.iter().copied().fold(f64::INFINITY, f64::min)
    })
}

pub fn stdev(source: &[f64], length: usize) -> Vec<f64> {
    rolling(source, length, |window| {
        let mean = window.iter().sum::<f64>() / length as f64;
        let var = window.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / length as f64;
        var.sqrt()
    })
}

fn rolling<F>(source: &[f64], length: usize, reducer: F) -> Vec<f64>
where
    F: Fn(&[f64]) -> f64,
{
    if length == 0 {
        return vec![f64::NAN; source.len()];
    }
    let mut out = vec![f64::NAN; source.len()];
    for i in length - 1..source.len() {
        let window = &source[i + 1 - length..=i];
        if window.iter().all(|v| v.is_finite()) {
            out[i] = reducer(window);
        }
    }
    out
}

pub fn ema(source: &[f64], length: usize) -> Vec<f64> {
    ewm(source, 2.0 / (length as f64 + 1.0), length)
}

pub fn rma(source: &[f64], length: usize) -> Vec<f64> {
    ewm(source, 1.0 / length as f64, length)
}

fn ewm(source: &[f64], alpha: f64, min_periods: usize) -> Vec<f64> {
    let mut out = vec![f64::NAN; source.len()];
    let mut state = f64::NAN;
    let mut seen = 0_usize;
    for (i, x) in source.iter().copied().enumerate() {
        if x.is_nan() {
            continue;
        }
        seen += 1;
        if state.is_nan() {
            state = x;
        } else {
            state = alpha * x + (1.0 - alpha) * state;
        }
        if seen >= min_periods {
            out[i] = state;
        }
    }
    out
}

pub fn rsi(source: &[f64], length: usize) -> Vec<f64> {
    let mut delta = vec![f64::NAN; source.len()];
    for i in 1..source.len() {
        delta[i] = source[i] - source[i - 1];
    }
    let gains: Vec<f64> = delta
        .iter()
        .map(|v| if v.is_nan() { f64::NAN } else { v.max(0.0) })
        .collect();
    let losses: Vec<f64> = delta
        .iter()
        .map(|v| if v.is_nan() { f64::NAN } else { -v.min(0.0) })
        .collect();
    let avg_gain = rma(&gains, length);
    let avg_loss = rma(&losses, length);
    avg_gain
        .iter()
        .zip(avg_loss.iter())
        .map(|(gain, loss)| {
            if gain.is_nan() || loss.is_nan() {
                f64::NAN
            } else if *loss == 0.0 && *gain > 0.0 {
                100.0
            } else if *loss == 0.0 {
                f64::NAN
            } else {
                let rs = gain / loss;
                100.0 - (100.0 / (1.0 + rs))
            }
        })
        .collect()
}

pub fn atr(bars: &Bars, length: usize) -> Vec<f64> {
    let mut tr = Vec::with_capacity(bars.len());
    for (i, bar) in bars.rows.iter().enumerate() {
        if i == 0 {
            tr.push((bar.high - bar.low).abs());
            continue;
        }
        let prev_close = bars.rows[i - 1].close;
        tr.push(
            (bar.high - bar.low)
                .abs()
                .max((bar.high - prev_close).abs())
                .max((bar.low - prev_close).abs()),
        );
    }
    rma(&tr, length)
}

pub fn crossover(a: &[f64], b: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; a.len().min(b.len())];
    for i in 1..out.len() {
        out[i] = (a[i] > b[i] && a[i - 1] <= b[i - 1]) as i32 as f64;
    }
    out
}

pub fn crossunder(a: &[f64], b: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; a.len().min(b.len())];
    for i in 1..out.len() {
        out[i] = (a[i] < b[i] && a[i - 1] >= b[i - 1]) as i32 as f64;
    }
    out
}

pub fn bollinger_bands(source: &[f64], length: usize, mult: f64) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let mid = sma(source, length);
    let sd = stdev(source, length);
    let upper: Vec<f64> = mid.iter().zip(&sd).map(|(m, s)| m + mult * s).collect();
    let lower: Vec<f64> = mid.iter().zip(&sd).map(|(m, s)| m - mult * s).collect();
    (lower, mid, upper)
}

pub fn supertrend_dir(bars: &Bars, length: usize, multiplier: f64) -> Vec<f64> {
    let atr_values = atr(bars, length);
    let mut upper = vec![f64::NAN; bars.len()];
    let mut lower = vec![f64::NAN; bars.len()];
    let mut direction = vec![f64::NAN; bars.len()];
    for i in 0..bars.len() {
        let bar = &bars.rows[i];
        let hl2 = (bar.high + bar.low) / 2.0;
        let basic_upper = hl2 + multiplier * atr_values[i];
        let basic_lower = hl2 - multiplier * atr_values[i];
        if i == 0 || atr_values[i].is_nan() {
            upper[i] = basic_upper;
            lower[i] = basic_lower;
            direction[i] = 1.0;
            continue;
        }
        upper[i] = if basic_upper < upper[i - 1] || bars.rows[i - 1].close > upper[i - 1] {
            basic_upper
        } else {
            upper[i - 1]
        };
        lower[i] = if basic_lower > lower[i - 1] || bars.rows[i - 1].close < lower[i - 1] {
            basic_lower
        } else {
            lower[i - 1]
        };
        direction[i] = if direction[i - 1] < 0.0 {
            if bar.close > upper[i] { 1.0 } else { -1.0 }
        } else if bar.close < lower[i] {
            -1.0
        } else {
            1.0
        };
    }
    direction
}
