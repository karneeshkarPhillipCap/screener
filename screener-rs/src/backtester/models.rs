use crate::backtester::slippage::SlippageModel;
use crate::data::Bars;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExitReason {
    Stop,
    Target,
    Trail,
    Time,
    ExitExpr,
    Eod,
}

impl std::fmt::Display for ExitReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            ExitReason::Stop => "stop",
            ExitReason::Target => "target",
            ExitReason::Trail => "trail",
            ExitReason::Time => "time",
            ExitReason::ExitExpr => "exit_expr",
            ExitReason::Eod => "eod",
        };
        f.write_str(text)
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntryOrderType {
    #[default]
    Moo,
    Moc,
    Limit,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PriceAdjustment {
    #[default]
    Full,
    SplitsOnly,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BacktestConfig {
    pub market: String,
    pub as_of: NaiveDate,
    pub hold: usize,
    pub top: usize,
    pub entry_expr: String,
    #[serde(default)]
    pub exit_expr: Option<String>,
    #[serde(default)]
    pub stop_loss: Option<f64>,
    #[serde(default)]
    pub take_profit: Option<f64>,
    #[serde(default)]
    pub trailing_stop: Option<f64>,
    #[serde(default)]
    pub slippage_bps: f64,
    #[serde(default)]
    pub commission_bps: f64,
    #[serde(default = "default_initial_capital")]
    pub initial_capital: f64,
    #[serde(default = "default_benchmark")]
    pub benchmark: String,
    #[serde(default)]
    pub strategy_name: Option<String>,
    #[serde(default)]
    pub tickers: Option<Vec<String>>,
    #[serde(default)]
    pub universe_file: Option<String>,
    #[serde(default = "default_max_universe")]
    pub max_universe: usize,
    #[serde(default)]
    pub min_price: Option<f64>,
    #[serde(default)]
    pub min_avg_dollar_volume: Option<f64>,
    #[serde(default = "default_adv_window")]
    pub avg_dollar_volume_window: usize,
    #[serde(default = "default_reserve_multiple")]
    pub reserve_multiple: usize,
    #[serde(default = "default_true")]
    pub reinvest: bool,
    #[serde(default)]
    pub slippage_model: SlippageModel,
    #[serde(default = "default_true")]
    pub gap_fills: bool,
    #[serde(default)]
    pub entry_order_type: EntryOrderType,
    #[serde(default)]
    pub entry_limit_bps: Option<f64>,
    #[serde(default)]
    pub allow_reentry: bool,
    #[serde(default)]
    pub max_reentries: usize,
    #[serde(default)]
    pub partial_exits: Vec<(f64, f64)>,
    #[serde(default)]
    pub price_adjustment: PriceAdjustment,
}

fn default_initial_capital() -> f64 {
    100_000.0
}
fn default_benchmark() -> String {
    "SPY".to_string()
}
fn default_max_universe() -> usize {
    200
}
fn default_adv_window() -> usize {
    20
}
fn default_reserve_multiple() -> usize {
    3
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Position {
    pub ticker: String,
    pub entry_date: NaiveDate,
    pub entry_fill: f64,
    pub shares: f64,
    pub slot_capital: f64,
    pub peak_price: f64,
    #[serde(default)]
    pub dividend_income: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Trade {
    pub ticker: String,
    pub rank: usize,
    pub signal_date: NaiveDate,
    pub entry_date: NaiveDate,
    pub entry_price: f64,
    pub exit_date: NaiveDate,
    pub exit_price: f64,
    pub exit_reason: ExitReason,
    pub shares: f64,
    pub entry_cost: f64,
    pub exit_value: f64,
    pub pnl: f64,
    pub return_pct: f64,
    #[serde(default)]
    pub dividend_income: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelectionRow {
    pub ticker: String,
    #[serde(default)]
    pub signal_date: Option<NaiveDate>,
    pub as_of_close: f64,
    pub as_of_volume: f64,
    pub as_of_dollar_vol: f64,
    pub rank: usize,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BacktestResult {
    pub config: BacktestConfig,
    pub trades: Vec<Trade>,
    pub equity_curve: Vec<(NaiveDate, f64)>,
    pub benchmark_curve: Vec<(NaiveDate, f64)>,
    pub metrics: BTreeMap<String, f64>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub selection: Vec<SelectionRow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimOutcome {
    pub trade: Option<Trade>,
    pub warning: Option<String>,
}

pub type BarsByTicker = BTreeMap<String, Bars>;
