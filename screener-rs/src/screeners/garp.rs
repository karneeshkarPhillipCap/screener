use crate::screeners::models::ScreenRow;

#[derive(Debug, Clone)]
pub struct GarpThresholds {
    pub min_market_cap: f64,
    pub min_sales: f64,
    pub max_peg: f64,
    pub min_sales_growth_5y: f64,
    pub min_operating_profit_growth: f64,
    pub min_eps_growth_5y: f64,
    pub min_roe_5y: f64,
    pub min_roce_or_roic: f64,
    pub min_quarterly_profit_growth: f64,
}

pub fn india_thresholds() -> GarpThresholds {
    GarpThresholds {
        min_market_cap: 1000.0,
        min_sales: 1000.0,
        max_peg: 2.0,
        min_sales_growth_5y: 15.0,
        min_operating_profit_growth: 10.0,
        min_eps_growth_5y: 12.0,
        min_roe_5y: 15.0,
        min_roce_or_roic: 15.0,
        min_quarterly_profit_growth: 0.0,
    }
}

pub fn passes_garp(row: &ScreenRow, thresholds: &GarpThresholds) -> bool {
    row.numeric("market_cap")
        .is_some_and(|v| v > thresholds.min_market_cap)
        && row
            .numeric("sales")
            .is_some_and(|v| v > thresholds.min_sales)
        && row
            .numeric("peg")
            .is_some_and(|v| v > 0.0 && v < thresholds.max_peg)
        && row
            .numeric("sales_growth_5y")
            .is_some_and(|v| v > thresholds.min_sales_growth_5y)
        && row
            .numeric("operating_profit_growth")
            .is_some_and(|v| v > thresholds.min_operating_profit_growth)
        && row
            .numeric("eps_growth_5y")
            .is_some_and(|v| v > thresholds.min_eps_growth_5y)
        && row
            .numeric("roe_5y")
            .is_some_and(|v| v > thresholds.min_roe_5y)
        && row
            .numeric("roce_or_roic")
            .is_some_and(|v| v > thresholds.min_roce_or_roic)
        && row
            .numeric("quarterly_profit_growth")
            .is_some_and(|v| v > thresholds.min_quarterly_profit_growth)
}

pub fn add_garp_score(rows: &[ScreenRow]) -> Vec<ScreenRow> {
    let mut out = rows.to_vec();
    if out.is_empty() {
        return out;
    }
    let peg = out.iter().map(|row| row.numeric("peg")).collect::<Vec<_>>();
    let inv_peg = rank_pct(&peg)
        .into_iter()
        .map(|rank| if rank == 0.0 { 0.0 } else { 1.0 - rank })
        .collect::<Vec<_>>();
    let eps = rank_pct(
        &out.iter()
            .map(|row| row.numeric("eps_growth_5y"))
            .collect::<Vec<_>>(),
    );
    let sales = rank_pct(
        &out.iter()
            .map(|row| row.numeric("sales_growth_5y"))
            .collect::<Vec<_>>(),
    );
    let roe = rank_pct(
        &out.iter()
            .map(|row| row.numeric("roe_5y"))
            .collect::<Vec<_>>(),
    );
    let roce = rank_pct(
        &out.iter()
            .map(|row| row.numeric("roce_or_roic"))
            .collect::<Vec<_>>(),
    );
    let quarterly = rank_pct(
        &out.iter()
            .map(|row| row.numeric("quarterly_profit_growth"))
            .collect::<Vec<_>>(),
    );
    for (idx, row) in out.iter_mut().enumerate() {
        let score = 30.0 * inv_peg[idx]
            + 20.0 * eps[idx]
            + 15.0 * sales[idx]
            + 15.0 * roe[idx]
            + 10.0 * roce[idx]
            + 10.0 * quarterly[idx];
        row.set_numeric("garp_score", (score * 100.0).round() / 100.0);
    }
    out.sort_by(|a, b| {
        b.numeric("garp_score")
            .unwrap_or(f64::NEG_INFINITY)
            .partial_cmp(&a.numeric("garp_score").unwrap_or(f64::NEG_INFINITY))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

fn rank_pct(values: &[Option<f64>]) -> Vec<f64> {
    let mut present = values
        .iter()
        .enumerate()
        .filter_map(|(idx, value)| value.filter(|v| v.is_finite()).map(|v| (idx, v)))
        .collect::<Vec<_>>();
    present.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let n = present.len();
    let mut out = vec![0.0; values.len()];
    if n == 0 {
        return out;
    }
    let mut i = 0;
    while i < present.len() {
        let start = i;
        let value = present[i].1;
        while i + 1 < present.len() && (present[i + 1].1 - value).abs() < f64::EPSILON {
            i += 1;
        }
        let end = i;
        let avg_rank = (start + 1 + end + 1) as f64 / 2.0;
        let pct = avg_rank / n as f64;
        for j in start..=end {
            out[present[j].0] = pct;
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn row(name: &str, peg: f64, eps_growth_5y: f64) -> ScreenRow {
        let mut fields = BTreeMap::new();
        for (key, value) in [
            ("market_cap", 1500.0),
            ("sales", 1600.0),
            ("peg", peg),
            ("sales_growth_5y", 18.0),
            ("operating_profit_growth", 12.0),
            ("eps_growth_5y", eps_growth_5y),
            ("roe_5y", 17.0),
            ("roce_or_roic", 18.0),
            ("quarterly_profit_growth", 20.0),
        ] {
            fields.insert(key.to_string(), serde_yaml::Value::from(value));
        }
        ScreenRow {
            ticker: Some(name.to_string()),
            name: Some(name.to_string()),
            description: None,
            fields,
        }
    }

    #[test]
    fn garp_filter_accepts_complete_india_match() {
        assert!(passes_garp(&row("AAA", 1.2, 16.0), &india_thresholds()));
    }

    #[test]
    fn garp_score_prefers_lower_peg_and_growth() {
        let scored = add_garp_score(&[row("LOWPEG", 0.8, 20.0), row("HIGHPEG", 1.8, 13.0)]);
        assert_eq!(scored[0].name.as_deref(), Some("LOWPEG"));
        assert!(scored[0].numeric("garp_score").is_some());
    }
}
