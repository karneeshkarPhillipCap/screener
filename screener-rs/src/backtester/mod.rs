pub mod engine;
pub mod metrics;
pub mod models;
pub mod optimization;
pub mod portfolio;
pub mod slippage;

pub use engine::{PriceFetcher, run_backtest, run_rolling_backtest, simulate_ticker};
pub use metrics::compute_metrics;
pub use models::{
    BacktestConfig, BacktestResult, EntryOrderType, ExitReason, Position, PriceAdjustment, Trade,
};
pub use portfolio::{Portfolio, build_equity_curve};
