use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bar {
    pub date: NaiveDate,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    #[serde(default)]
    pub adj_close: Option<f64>,
    #[serde(default)]
    pub dividend: Option<f64>,
}

impl Bar {
    pub fn value(&self, name: &str) -> Option<f64> {
        match name {
            "open" => Some(self.open),
            "high" => Some(self.high),
            "low" => Some(self.low),
            "close" => Some(self.close),
            "volume" => Some(self.volume),
            "adj_close" => Some(self.adj_close.unwrap_or(self.close)),
            "dividend" => Some(self.dividend.unwrap_or(0.0)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Bars {
    pub rows: Vec<Bar>,
}

impl Bars {
    pub fn new(mut rows: Vec<Bar>) -> Self {
        rows.sort_by_key(|bar| bar.date);
        rows.dedup_by_key(|bar| bar.date);
        Self { rows }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn get(&self, idx: usize) -> Option<&Bar> {
        self.rows.get(idx)
    }

    pub fn dates(&self) -> Vec<NaiveDate> {
        self.rows.iter().map(|bar| bar.date).collect()
    }

    pub fn series(&self, name: &str) -> Option<Vec<f64>> {
        self.rows.iter().map(|bar| bar.value(name)).collect()
    }

    pub fn position_on_or_before(&self, day: NaiveDate) -> Option<usize> {
        match self.rows.binary_search_by_key(&day, |bar| bar.date) {
            Ok(idx) => Some(idx),
            Err(0) => None,
            Err(idx) => Some(idx - 1),
        }
    }

    pub fn position(&self, day: NaiveDate) -> Option<usize> {
        self.rows.binary_search_by_key(&day, |bar| bar.date).ok()
    }

    pub fn slice_through(&self, idx: usize) -> Self {
        Self {
            rows: self.rows[..=idx.min(self.rows.len().saturating_sub(1))].to_vec(),
        }
    }

    pub fn between(&self, start: NaiveDate, end: NaiveDate) -> Self {
        Self {
            rows: self
                .rows
                .iter()
                .filter(|bar| bar.date >= start && bar.date <= end)
                .cloned()
                .collect(),
        }
    }
}

pub type PricePanel = BTreeMap<String, Bars>;

pub fn tv_to_yf(symbol: &str, market: &str) -> String {
    let sym = symbol.trim().to_uppercase();
    if let Some((exchange, rest)) = sym.split_once(':') {
        return match exchange {
            "NSE" => format!("{rest}.NS"),
            "BSE" => format!("{rest}.BO"),
            _ => rest.to_string(),
        };
    }
    if market == "india" && !sym.contains('.') {
        return format!("{sym}.NS");
    }
    sym
}

pub fn business_days(start: NaiveDate, end: NaiveDate) -> Vec<NaiveDate> {
    use chrono::Datelike;

    let mut out = Vec::new();
    let mut day = start;
    while day <= end {
        let weekday = day.weekday().number_from_monday();
        if weekday <= 5 {
            out.push(day);
        }
        day = day.succ_opt().expect("date overflow");
    }
    out
}

pub fn read_price_csv(path: impl AsRef<Path>) -> anyhow::Result<PricePanel> {
    #[derive(Debug, Deserialize)]
    struct Row {
        ticker: String,
        date: NaiveDate,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        #[serde(default)]
        adj_close: Option<f64>,
        #[serde(default)]
        dividend: Option<f64>,
    }

    let mut rdr = csv::Reader::from_path(path)?;
    let mut grouped: BTreeMap<String, Vec<Bar>> = BTreeMap::new();
    for row in rdr.deserialize::<Row>() {
        let row = row?;
        grouped.entry(row.ticker).or_default().push(Bar {
            date: row.date,
            open: row.open,
            high: row.high,
            low: row.low,
            close: row.close,
            volume: row.volume,
            adj_close: row.adj_close,
            dividend: row.dividend,
        });
    }
    Ok(grouped
        .into_iter()
        .map(|(ticker, rows)| (ticker, Bars::new(rows)))
        .collect())
}
