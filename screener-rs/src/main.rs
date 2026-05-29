use anyhow::Context;
use chrono::NaiveDate;
use clap::{Args, Parser, Subcommand};
use screener_rs::backtester::models::{BacktestConfig, EntryOrderType, PriceAdjustment};
use screener_rs::backtester::slippage::SlippageModel;
use screener_rs::backtester::{PriceFetcher, run_backtest, run_rolling_backtest};
use screener_rs::config::{CliConfig, built_in_strategy, load_yaml_config};
use screener_rs::data::{PricePanel, read_price_csv, tv_to_yf};
use screener_rs::providers::nse::NseClient;
use screener_rs::providers::tradingview::TradingViewClient;
use screener_rs::providers::yahoo::YahooPriceFetcher;
use screener_rs::screeners::engine::{ScreenRequest, screen_rows};
use screener_rs::screeners::garp::{add_garp_score, india_thresholds, passes_garp};
use screener_rs::screeners::insiders::{PromoterBuyRequest, screen_promoter_buys};
use screener_rs::screeners::models::{read_screen_csv, write_screen_csv};
use screener_rs::screeners::operator;
use screener_rs::screeners::rs_breakout::{RsBreakoutResult, scan_rs_breakouts};
use screener_rs::screeners::unusual_volume::{
    DEFAULT_MIN_RVOL, DEFAULT_MIN_Z, UnusualVolumeResult, UnusualVolumeScanRequest,
    run_unusual_volume_scan,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "screener-rs")]
#[command(about = "Rust migration target for the screener CLI")]
struct Cli {
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[arg(long, global = true, default_value = "INFO")]
    log_level: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(name = "backtest-historical")]
    BacktestHistorical(BacktestHistoricalArgs),
    #[command(name = "backtest-rolling")]
    BacktestRolling(BacktestRollingArgs),
    #[command(name = "screen")]
    Screen(ScreenArgs),
    #[command(name = "garp")]
    Garp(GarpArgs),
    #[command(name = "promoter-buys")]
    PromoterBuys(PromoterBuysArgs),
    #[command(name = "rs-breakout")]
    RsBreakout(RsBreakoutArgs),
    #[command(name = "unusual-volume")]
    UnusualVolume(UnusualVolumeArgs),
    #[command(name = "operator-scan")]
    OperatorScan(OperatorScanArgs),
    #[command(name = "earnings-backtest")]
    EarningsBacktest(PlaceholderArgs),
    #[command(name = "vbt-sweep")]
    VbtSweep(PlaceholderArgs),
    #[command(name = "backtest-lab")]
    BacktestLab(PlaceholderArgs),
    #[command(name = "optimize")]
    Optimize(PlaceholderArgs),
    #[command(name = "usage-report")]
    UsageReport(PlaceholderArgs),
}

#[derive(Debug, Args)]
struct PlaceholderArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[derive(Debug, Args)]
struct ScreenArgs {
    #[arg(short = 'm', long, default_value = "us")]
    market: String,
    #[arg(short = 'c', long = "criteria")]
    criteria_names: Vec<String>,
    #[arg(short = 'n', long, default_value_t = 50)]
    limit: usize,
    #[arg(long = "sort", default_value = "setup_score")]
    order_by: String,
    #[arg(long)]
    csv: bool,
    #[arg(long)]
    detail: bool,
    #[arg(long)]
    refresh: bool,
    #[arg(long, default_value = "15m")]
    cache_ttl: String,
    #[arg(long)]
    input_csv: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct GarpArgs {
    #[arg(short = 'm', long, default_value = "india")]
    market: String,
    #[arg(short = 'n', long, default_value_t = 30)]
    limit: usize,
    #[arg(long)]
    csv: bool,
    #[arg(long)]
    refresh: bool,
    #[arg(long, default_value = "15m")]
    cache_ttl: String,
    #[arg(long)]
    input_csv: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct PromoterBuysArgs {
    #[arg(short = 'm', long, default_value = "india")]
    market: String,
    #[arg(long, default_value_t = 200)]
    universe_size: usize,
    #[arg(short = 'n', long, default_value_t = 30)]
    limit: usize,
    #[arg(long = "min-change", default_value_t = 0.0)]
    min_change_pct: f64,
    #[arg(long)]
    min_yf_net_pct: Option<f64>,
    #[arg(long)]
    require_both: bool,
    #[arg(long)]
    min_market_cap: Option<f64>,
    #[arg(long, default_value_t = 10)]
    workers: usize,
    #[arg(long)]
    csv: bool,
    #[arg(long)]
    refresh: bool,
    #[arg(long, default_value = "15m")]
    cache_ttl: String,
    #[arg(long)]
    input_csv: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct RsBreakoutArgs {
    #[arg(short = 'm', long, default_value = "india")]
    market: String,
    #[arg(long)]
    as_of: Option<NaiveDate>,
    #[arg(long)]
    tickers: Option<String>,
    #[arg(long)]
    universe_file: Option<PathBuf>,
    #[arg(long, default_value_t = 500)]
    universe_limit: usize,
    #[arg(long)]
    benchmark: Option<String>,
    #[arg(long, default_value_t = 220)]
    history_days: i64,
    #[arg(short = 'n', long, default_value_t = 50)]
    limit: usize,
    #[arg(long)]
    json: Option<PathBuf>,
    #[arg(long)]
    md: Option<PathBuf>,
    #[arg(long)]
    refresh: bool,
    #[arg(long, default_value = "15m")]
    cache_ttl: String,
    #[arg(long)]
    no_output_files: bool,
    #[arg(long)]
    prices_csv: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct UnusualVolumeArgs {
    #[arg(short = 'm', long, default_value = "us")]
    market: String,
    #[arg(long)]
    as_of: Option<NaiveDate>,
    #[arg(long)]
    tickers: Option<String>,
    #[arg(long)]
    universe_file: Option<PathBuf>,
    #[arg(long, default_value_t = 500)]
    universe_limit: usize,
    #[arg(long, default_value_t = DEFAULT_MIN_RVOL)]
    min_rvol: f64,
    #[arg(long, default_value_t = DEFAULT_MIN_Z)]
    min_z: f64,
    #[arg(long, default_value = "moderate")]
    strength: String,
    #[arg(long, default_value_t = 100_000.0)]
    min_avg_volume: f64,
    #[arg(long)]
    min_market_cap: Option<f64>,
    #[arg(long)]
    include_fno_ban: bool,
    #[arg(long)]
    deep_india: bool,
    #[arg(long)]
    option_chain: bool,
    #[arg(long)]
    fii_dii: bool,
    #[arg(long)]
    pledge: bool,
    #[arg(long)]
    json: Option<PathBuf>,
    #[arg(long)]
    md: Option<PathBuf>,
    #[arg(long)]
    no_output_files: bool,
    #[arg(long)]
    refresh: bool,
    #[arg(short = 'n', long, default_value_t = 50)]
    limit: usize,
    #[arg(long = "buildup", default_value_t = false)]
    buildup_enabled: bool,
    #[arg(long, default_value_t = 20)]
    buildup_window: usize,
    #[arg(long, default_value_t = 0.6)]
    buildup_min_score: f64,
    #[arg(long)]
    prices_csv: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct OperatorScanArgs {
    #[arg(long = "date")]
    as_of: Option<NaiveDate>,
    #[arg(long, default_value = "fo+cash")]
    universe: String,
    #[arg(long = "output")]
    out_path: Option<PathBuf>,
    #[arg(long)]
    only_actions: bool,
    #[arg(short = 'v', long)]
    verbose: bool,
}

#[derive(Debug, Args, Clone)]
struct CommonBacktestArgs {
    #[arg(short = 'm', long, default_value = "us")]
    market: String,
    #[arg(long, default_value_t = 20)]
    hold: usize,
    #[arg(long, default_value_t = 10)]
    top: usize,
    #[arg(long = "entry")]
    entry_expr: Option<String>,
    #[arg(long = "exit")]
    exit_expr: Option<String>,
    #[arg(long = "strategy")]
    strategy_name: Option<String>,
    #[arg(long)]
    stop_loss: Option<f64>,
    #[arg(long)]
    take_profit: Option<f64>,
    #[arg(long)]
    trailing_stop: Option<f64>,
    #[arg(long, default_value_t = 0.0)]
    slippage_bps: f64,
    #[arg(long, default_value_t = 0.0)]
    commission_bps: f64,
    #[arg(long, default_value_t = 100_000.0)]
    initial_capital: f64,
    #[arg(long)]
    benchmark: Option<String>,
    #[arg(long)]
    tickers: Option<String>,
    #[arg(long)]
    universe_file: Option<String>,
    #[arg(long, default_value_t = 200)]
    max_universe: usize,
    #[arg(long)]
    min_price: Option<f64>,
    #[arg(long)]
    min_avg_dollar_volume: Option<f64>,
    #[arg(long, default_value_t = 20)]
    adv_window: usize,
    #[arg(long, default_value_t = 3)]
    reserve_multiple: usize,
    #[arg(long)]
    no_reinvest: bool,
    #[arg(long, default_value = "fixed")]
    slippage_model: String,
    #[arg(long, default_value_t = 0.0)]
    half_spread_bps: f64,
    #[arg(long, default_value_t = 0.1)]
    vol_impact_k: f64,
    #[arg(long)]
    no_gap_fills: bool,
    #[arg(long, default_value = "moo")]
    entry_order: String,
    #[arg(long)]
    entry_limit_bps: Option<f64>,
    #[arg(long)]
    allow_reentry: bool,
    #[arg(long, default_value_t = 0)]
    max_reentries: usize,
    #[arg(long = "partial-exit")]
    partial_exit_args: Vec<String>,
    #[arg(long, default_value = "full")]
    price_adjustment: String,
    #[arg(long)]
    csv: bool,
    #[arg(long)]
    prices_csv: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct BacktestHistoricalArgs {
    #[command(flatten)]
    common: CommonBacktestArgs,
    #[arg(long)]
    as_of: NaiveDate,
}

#[derive(Debug, Args)]
struct BacktestRollingArgs {
    #[command(flatten)]
    common: CommonBacktestArgs,
    #[arg(long)]
    start: Option<NaiveDate>,
    #[arg(long)]
    end: Option<NaiveDate>,
    #[arg(long, default_value_t = 1)]
    years: i64,
}

struct CsvFetcher {
    panel: PricePanel,
}

impl PriceFetcher for CsvFetcher {
    fn fetch(
        &self,
        tickers: &[String],
        _start: NaiveDate,
        _end: NaiveDate,
    ) -> anyhow::Result<PricePanel> {
        Ok(tickers
            .iter()
            .map(|ticker| {
                (
                    ticker.clone(),
                    self.panel.get(ticker).cloned().unwrap_or_default(),
                )
            })
            .collect())
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let cfg = match &cli.config {
        Some(path) => load_yaml_config(path)
            .with_context(|| format!("failed to load YAML config {}", path.display()))?,
        None => CliConfig::default(),
    };

    match cli.command {
        Command::BacktestHistorical(args) => {
            let output_csv = args.common.csv;
            let fetcher = build_price_fetcher(args.common.prices_csv.as_ref())?;
            let cfg = build_backtest_config(args.as_of, args.common, &cfg)?;
            let result = run_backtest(cfg, fetcher.as_ref())?;
            if output_csv {
                print_trade_csv(&result);
            } else {
                print_backtest_result(&result);
            }
        }
        Command::BacktestRolling(args) => {
            let output_csv = args.common.csv;
            let fetcher = build_price_fetcher(args.common.prices_csv.as_ref())?;
            let end = args.end.unwrap_or_else(|| chrono::Utc::now().date_naive());
            let start = args
                .start
                .unwrap_or_else(|| end - chrono::Duration::days(365 * args.years));
            let cfg = build_backtest_config(end, args.common, &cfg)?;
            let result = run_rolling_backtest(cfg, fetcher.as_ref(), start, end)?;
            if output_csv {
                print_trade_csv(&result);
            } else {
                print_backtest_result(&result);
            }
        }
        Command::Screen(args) => {
            let _provider_options = (args.refresh, args.cache_ttl.clone());
            let criteria_names = if args.criteria_names.is_empty() {
                vec!["ema".to_string()]
            } else {
                args.criteria_names
            };
            if criteria_names.len() == 1 && args.input_csv.is_none() {
                match criteria_names[0].as_str() {
                    "promoter-buys" => {
                        let promoter_args = PromoterBuysArgs {
                            market: args.market,
                            universe_size: args.limit.max(200),
                            limit: args.limit,
                            min_change_pct: 0.0,
                            min_yf_net_pct: None,
                            require_both: false,
                            min_market_cap: None,
                            workers: 10,
                            csv: args.csv,
                            refresh: false,
                            cache_ttl: args.cache_ttl,
                            input_csv: None,
                        };
                        run_promoter_buys(promoter_args)?;
                        return Ok(());
                    }
                    "rs-breakout" => {
                        let rs_args = RsBreakoutArgs {
                            market: args.market,
                            as_of: None,
                            tickers: None,
                            universe_file: None,
                            universe_limit: args.limit.max(500),
                            benchmark: None,
                            history_days: 220,
                            limit: args.limit,
                            json: None,
                            md: None,
                            refresh: false,
                            cache_ttl: args.cache_ttl,
                            no_output_files: true,
                            prices_csv: None,
                        };
                        run_rs_breakout(rs_args)?;
                        return Ok(());
                    }
                    "unusual-volume" => {
                        let uv_args = UnusualVolumeArgs {
                            market: args.market,
                            as_of: None,
                            tickers: None,
                            universe_file: None,
                            universe_limit: args.limit.max(500),
                            min_rvol: DEFAULT_MIN_RVOL,
                            min_z: DEFAULT_MIN_Z,
                            strength: "moderate".to_string(),
                            min_avg_volume: 100_000.0,
                            min_market_cap: None,
                            include_fno_ban: false,
                            deep_india: false,
                            option_chain: false,
                            fii_dii: false,
                            pledge: false,
                            json: None,
                            md: None,
                            no_output_files: true,
                            refresh: false,
                            limit: args.limit,
                            buildup_enabled: false,
                            buildup_window: 20,
                            buildup_min_score: 0.6,
                            prices_csv: None,
                        };
                        run_unusual_volume(uv_args)?;
                        return Ok(());
                    }
                    _ => {}
                }
            }
            let rows = match &args.input_csv {
                Some(path) => read_screen_csv(path)?,
                None => TradingViewClient::new()?.screen_rows(
                    &args.market,
                    &criteria_names,
                    &args.order_by,
                    args.limit,
                    args.detail,
                )?,
            };
            let result = screen_rows(
                &rows,
                &ScreenRequest {
                    market: args.market,
                    criteria_names,
                    limit: args.limit,
                    order_by: args.order_by,
                    detail: args.detail,
                },
            )?;
            if args.csv {
                write_screen_csv(&result)?;
            } else {
                print_screen_rows(&result);
            }
        }
        Command::Garp(args) => {
            let _provider_options = (args.refresh, args.cache_ttl.clone());
            let mut rows = match &args.input_csv {
                Some(path) => read_screen_csv(path)?,
                None => anyhow::bail!(
                    "Rust live GARP still needs an input CSV with GARP columns; live screener.in/yfinance fundamental enrichment is not ported yet"
                ),
            };
            if args.market == "india" {
                let thresholds = india_thresholds();
                rows.retain(|row| passes_garp(row, &thresholds));
            }
            let mut result = add_garp_score(&rows);
            result.truncate(args.limit);
            if args.csv {
                write_screen_csv(&result)?;
            } else {
                print_screen_rows(&result);
            }
        }
        Command::PromoterBuys(args) => run_promoter_buys(args)?,
        Command::RsBreakout(args) => run_rs_breakout(args)?,
        Command::UnusualVolume(args) => run_unusual_volume(args)?,
        Command::OperatorScan(args) => run_operator_scan(args)?,
        other => {
            let name = match other {
                Command::EarningsBacktest(_) => "earnings-backtest",
                Command::VbtSweep(_) => "vbt-sweep",
                Command::BacktestLab(_) => "backtest-lab",
                Command::Optimize(_) => "optimize",
                Command::UsageReport(_) => "usage-report",
                Command::PromoterBuys(_)
                | Command::RsBreakout(_)
                | Command::UnusualVolume(_)
                | Command::OperatorScan(_) => unreachable!(),
                Command::BacktestHistorical(_) | Command::BacktestRolling(_) => unreachable!(),
                Command::Screen(_) | Command::Garp(_) => unreachable!(),
            };
            anyhow::bail!("{name} is registered in the Rust CLI shell but not ported yet");
        }
    }
    Ok(())
}

fn build_price_fetcher(path: Option<&PathBuf>) -> anyhow::Result<Box<dyn PriceFetcher>> {
    if let Some(path) = path {
        return Ok(Box::new(CsvFetcher {
            panel: read_price_csv(path)?,
        }));
    }
    Ok(Box::new(YahooPriceFetcher::new()?))
}

fn run_promoter_buys(args: PromoterBuysArgs) -> anyhow::Result<()> {
    let _provider_options = (args.refresh, args.cache_ttl.clone(), args.workers);
    let universe = match &args.input_csv {
        Some(path) => read_screen_csv(path)?,
        None => TradingViewClient::new()?.liquid_universe(
            &args.market,
            args.universe_size,
            if args.market == "india" { 10.0 } else { 1.0 },
            args.min_market_cap,
        )?,
    };
    let rows = screen_promoter_buys(
        &universe,
        &PromoterBuyRequest {
            market: args.market,
            limit: args.limit,
            min_change_pct: args.min_change_pct,
            min_yf_net_pct: args.min_yf_net_pct,
            require_both: args.require_both,
        },
    )?;
    if args.csv {
        write_screen_csv(&rows)?;
    } else {
        print_screen_rows(&rows);
    }
    Ok(())
}

fn run_rs_breakout(args: RsBreakoutArgs) -> anyhow::Result<()> {
    let _provider_options = (args.refresh, args.cache_ttl.clone());
    let as_of = args
        .as_of
        .unwrap_or_else(|| chrono::Utc::now().date_naive());
    let benchmark = args.benchmark.unwrap_or_else(|| {
        if args.market == "india" {
            "^NSEI"
        } else {
            "SPY"
        }
        .to_string()
    });
    let tickers = resolve_tickers(
        &args.market,
        args.tickers.as_deref(),
        args.universe_file.as_ref(),
        args.universe_limit,
        args.prices_csv.as_ref(),
        Some(&benchmark),
    )?;
    if tickers.is_empty() {
        anyhow::bail!("Empty universe: pass --tickers or --universe-file.");
    }
    eprintln!(
        "Scanning {} {} tickers as of {}...",
        tickers.len(),
        args.market.to_uppercase(),
        as_of
    );
    let start = as_of - chrono::Duration::days(args.history_days);
    let end = as_of + chrono::Duration::days(1);
    let fetcher = build_price_fetcher(args.prices_csv.as_ref())?;
    let bars_by_symbol =
        fetch_bars_for_symbols(fetcher.as_ref(), &tickers, &args.market, start, end)?;
    let benchmark_panel = fetcher.fetch(std::slice::from_ref(&benchmark), start, end)?;
    let benchmark_bars = benchmark_panel
        .get(&benchmark)
        .cloned()
        .or_else(|| {
            benchmark_panel
                .get(&tv_to_yf(&benchmark, &args.market))
                .cloned()
        })
        .unwrap_or_default();
    let delivery_panel = if args.market == "india" {
        let syms = tickers
            .iter()
            .map(|s| screener_rs::screeners::rs_breakout::india_symbol(s))
            .collect::<Vec<_>>();
        NseClient::new()?.load_delivery_panel(&syms, as_of, 14)
    } else {
        Vec::new()
    };
    let result = scan_rs_breakouts(
        &bars_by_symbol,
        &benchmark_bars,
        as_of,
        &delivery_panel,
        &benchmark,
        args.market == "india",
    )?;
    print_rs_breakout_result(&result, args.limit, &args.market);
    if !args.no_output_files {
        let json_path = args.json.unwrap_or_else(|| {
            PathBuf::from(format!("rs_breakout_{}_{}.json", args.market, result.as_of))
        });
        let md_path = args.md.unwrap_or_else(|| {
            PathBuf::from(format!("rs_breakout_{}_{}.md", args.market, result.as_of))
        });
        std::fs::write(&json_path, serde_json::to_string_pretty(&result)?)?;
        std::fs::write(&md_path, rs_breakout_markdown(&result, &args.market))?;
        eprintln!("Wrote {} + {}", json_path.display(), md_path.display());
    }
    Ok(())
}

fn run_unusual_volume(args: UnusualVolumeArgs) -> anyhow::Result<()> {
    let _provider_options = (args.refresh, args.buildup_window, args.buildup_min_score);
    if args.deep_india || args.option_chain || args.fii_dii || args.pledge || args.buildup_enabled {
        eprintln!(
            "Rust unusual-volume currently skips deep-india, option-chain, fii-dii, pledge, and buildup overlays."
        );
    }
    let as_of = args
        .as_of
        .unwrap_or_else(|| chrono::Utc::now().date_naive());
    let (tickers, market_caps) = resolve_unusual_universe(&args, as_of)?;
    if tickers.is_empty() {
        anyhow::bail!("Empty universe: pass --tickers or --universe-file.");
    }
    eprintln!(
        "Scanning {} {} tickers as of {}...",
        tickers.len(),
        args.market.to_uppercase(),
        as_of
    );
    let start = as_of - chrono::Duration::days(400);
    let end = as_of + chrono::Duration::days(1);
    let fetcher = build_price_fetcher(args.prices_csv.as_ref())?;
    let bars_by_symbol =
        fetch_bars_for_symbols(fetcher.as_ref(), &tickers, &args.market, start, end)?;
    let nse = if args.market == "india" {
        Some(NseClient::new()?)
    } else {
        None
    };
    let banned = if args.market == "india" && !args.include_fno_ban {
        nse.as_ref()
            .and_then(|client| client.fno_ban_list().ok())
            .unwrap_or_default()
    } else {
        Default::default()
    };
    let delivery_panel = if args.market == "india" {
        let syms = tickers
            .iter()
            .map(|s| screener_rs::screeners::rs_breakout::india_symbol(s))
            .collect::<Vec<_>>();
        nse.as_ref()
            .map(|client| client.load_delivery_panel(&syms, as_of, 40))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let min_market_cap = args.min_market_cap.unwrap_or_else(|| {
        if args.market == "india" {
            5_000_000_000.0
        } else {
            300_000_000.0
        }
    });
    let mut result = run_unusual_volume_scan(UnusualVolumeScanRequest {
        bars_by_symbol: &bars_by_symbol,
        as_of,
        min_rvol: args.min_rvol,
        min_z: args.min_z,
        strength_floor: &args.strength,
        min_avg_volume: args.min_avg_volume,
        min_market_cap,
        market_caps: &market_caps,
        delivery_panel: &delivery_panel,
        banned_symbols: &banned,
    });
    result.events.truncate(args.limit);
    print_unusual_volume_result(&result, &args.market, as_of);
    if !args.no_output_files && !result.events.is_empty() {
        let json_path = args.json.unwrap_or_else(|| {
            PathBuf::from(format!("unusual_volume_{}_{}.json", args.market, as_of))
        });
        let md_path = args.md.unwrap_or_else(|| {
            PathBuf::from(format!("unusual_volume_{}_{}.md", args.market, as_of))
        });
        std::fs::write(&json_path, serde_json::to_string_pretty(&result.events)?)?;
        std::fs::write(
            &md_path,
            unusual_volume_markdown(&result, &args.market, as_of),
        )?;
        eprintln!("Wrote {} + {}", json_path.display(), md_path.display());
    }
    Ok(())
}

fn run_operator_scan(args: OperatorScanArgs) -> anyhow::Result<()> {
    let _verbose = args.verbose;
    let as_of = args
        .as_of
        .unwrap_or_else(|| chrono::Utc::now().date_naive());
    let (rows, actual) = operator::build_dataset(as_of, &args.universe)?;
    let written = operator::write_csv(&rows, actual, args.out_path.as_deref(), args.only_actions)?;
    let mut actions = BTreeMap::<String, usize>::new();
    let mut high_momentum = 0_usize;
    for row in &rows {
        if let Some(action) = &row.operator_action {
            *actions.entry(action.clone()).or_default() += 1;
        }
        high_momentum += row.high_momentum_watch as usize;
    }
    println!(
        "Operator scan: trading day {}  ·  {} symbols  ·  wrote {}",
        actual,
        rows.len(),
        written.display()
    );
    for (action, count) in actions {
        println!("  {action:<16} {count}");
    }
    println!("  High_Momentum_Watch: {high_momentum}");
    Ok(())
}

fn resolve_tickers(
    market: &str,
    tickers: Option<&str>,
    universe_file: Option<&PathBuf>,
    universe_limit: usize,
    prices_csv: Option<&PathBuf>,
    excluded: Option<&str>,
) -> anyhow::Result<Vec<String>> {
    if let Some(raw) = tickers {
        return Ok(split_tickers(raw));
    }
    if let Some(path) = universe_file {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read universe file {}", path.display()))?;
        return Ok(text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect());
    }
    if let Some(path) = prices_csv {
        let panel = read_price_csv(path)?;
        let excluded = excluded.map(str::to_string);
        return Ok(panel
            .keys()
            .filter(|ticker| excluded.as_ref() != Some(*ticker))
            .cloned()
            .collect());
    }
    let rows = TradingViewClient::new()?.liquid_universe(
        market,
        universe_limit,
        if market == "india" { 50.0 } else { 5.0 },
        None,
    )?;
    Ok(rows
        .into_iter()
        .filter_map(|row| row.name.or(row.ticker))
        .collect())
}

fn resolve_unusual_universe(
    args: &UnusualVolumeArgs,
    _as_of: NaiveDate,
) -> anyhow::Result<(Vec<String>, BTreeMap<String, f64>)> {
    if args.tickers.is_some() || args.universe_file.is_some() || args.prices_csv.is_some() {
        let tickers = resolve_tickers(
            &args.market,
            args.tickers.as_deref(),
            args.universe_file.as_ref(),
            args.universe_limit,
            args.prices_csv.as_ref(),
            None,
        )?;
        return Ok((tickers, BTreeMap::new()));
    }
    let rows = TradingViewClient::new()?.liquid_universe(
        &args.market,
        args.universe_limit,
        if args.market == "india" { 50.0 } else { 5.0 },
        args.min_market_cap,
    )?;
    let mut tickers = Vec::new();
    let mut caps = BTreeMap::new();
    for row in rows {
        let Some(symbol) = row.name.clone().or(row.ticker.clone()) else {
            continue;
        };
        if let Some(cap) = row.numeric("market_cap_basic") {
            caps.insert(
                screener_rs::screeners::rs_breakout::india_symbol(&symbol),
                cap,
            );
        }
        tickers.push(symbol);
    }
    Ok((tickers, caps))
}

fn split_tickers(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn fetch_bars_for_symbols(
    fetcher: &dyn PriceFetcher,
    tickers: &[String],
    market: &str,
    start: NaiveDate,
    end: NaiveDate,
) -> anyhow::Result<PricePanel> {
    let yf_symbols = tickers
        .iter()
        .map(|ticker| tv_to_yf(ticker, market))
        .collect::<Vec<_>>();
    let raw_panel = fetcher.fetch(&yf_symbols, start, end)?;
    let mut out = PricePanel::new();
    for (ticker, yf) in tickers.iter().zip(yf_symbols.iter()) {
        let bars = raw_panel
            .get(yf)
            .cloned()
            .or_else(|| raw_panel.get(ticker).cloned())
            .unwrap_or_default();
        out.insert(ticker.clone(), bars);
    }
    Ok(out)
}

fn print_rs_breakout_result(result: &RsBreakoutResult, limit: usize, market: &str) {
    println!(
        "{} RS Breakout Screen as of {} vs {}",
        market.to_uppercase(),
        result.as_of,
        result.benchmark
    );
    print_rs_bucket("Full", &result.full, limit);
    print_rs_bucket(
        "Relaxed (without price breakout and delivery increase)",
        &result.relaxed,
        limit,
    );
}

fn print_rs_bucket(
    title: &str,
    rows: &[screener_rs::screeners::rs_breakout::RsBreakoutRow],
    limit: usize,
) {
    println!("{title} - {} match(es)", rows.len());
    println!(
        "symbol,close,rs_55,supertrend,previous_week_high,volume_ratio,delivery_pct,previous_delivery_pct"
    );
    for row in rows.iter().take(limit) {
        println!(
            "{},{:.2},{:.2},{:.2},{},{:.2},{},{}",
            row.symbol,
            row.close,
            row.rs_55,
            row.supertrend,
            fmt_opt(row.previous_week_high),
            row.volume_ratio,
            fmt_opt(row.delivery_pct),
            fmt_opt(row.previous_delivery_pct)
        );
    }
}

fn rs_breakout_markdown(result: &RsBreakoutResult, market: &str) -> String {
    let mut lines = vec![
        format!(
            "# {} RS Breakout Screen ({})",
            market.to_uppercase(),
            result.as_of
        ),
        String::new(),
        format!("**Benchmark:** {}", result.benchmark),
        String::new(),
    ];
    for (title, rows) in [
        ("Full", result.full.as_slice()),
        (
            "Relaxed (without price breakout and delivery increase)",
            result.relaxed.as_slice(),
        ),
    ] {
        lines.push(format!("## {title} ({})", rows.len()));
        lines.push(String::new());
        lines.push("| # | Ticker | Close | RS55 | SuperTrend | Prev Week High | Vol Ratio | Deliv% | Prev Deliv% |".to_string());
        lines.push("|---|--------|------:|-----:|-----------:|---------------:|----------:|-------:|------------:|".to_string());
        for (i, row) in rows.iter().enumerate() {
            lines.push(format!(
                "| {} | **{}** | {:.2} | {:.2} | {:.2} | {} | {:.2} | {} | {} |",
                i + 1,
                row.symbol,
                row.close,
                row.rs_55,
                row.supertrend,
                fmt_opt(row.previous_week_high),
                row.volume_ratio,
                fmt_opt(row.delivery_pct),
                fmt_opt(row.previous_delivery_pct)
            ));
        }
        lines.push(String::new());
    }
    lines.join("\n")
}

fn print_unusual_volume_result(result: &UnusualVolumeResult, market: &str, as_of: NaiveDate) {
    if result.events.is_empty() {
        println!(
            "No unusual-volume events on {} for {}. fetched={}, liquid={}",
            as_of,
            market.to_uppercase(),
            result.fetched_count,
            result.liquid_count
        );
        return;
    }
    println!("Unusual Volume - {} ({})", market.to_uppercase(), as_of);
    println!("symbol,strength,direction,close,pct_change,volume,rvol,z_score,delivery_pct,notes");
    for event in &result.events {
        println!(
            "{},{},{},{:.2},{:.2},{:.0},{:.2},{},{},{}",
            event.symbol,
            event.strength,
            event.direction,
            event.close,
            event.pct_change,
            event.volume,
            event.rvol,
            fmt_nan(event.z_score),
            fmt_opt(event.delivery_pct),
            event.notes
        );
    }
}

fn unusual_volume_markdown(result: &UnusualVolumeResult, market: &str, as_of: NaiveDate) -> String {
    let mut lines = vec![
        format!("# Unusual Volume - {} ({})", market.to_uppercase(), as_of),
        String::new(),
        "| # | Symbol | Strength | Direction | Close | % Chg | RVOL | Z | Deliv% | Notes |"
            .to_string(),
        "|---|--------|----------|-----------|------:|------:|-----:|--:|-------:|-------|"
            .to_string(),
    ];
    for (i, event) in result.events.iter().enumerate() {
        lines.push(format!(
            "| {} | **{}** | {} | {} | {:.2} | {:.2} | {:.2} | {} | {} | {} |",
            i + 1,
            event.symbol,
            event.strength,
            event.direction,
            event.close,
            event.pct_change,
            event.rvol,
            fmt_nan(event.z_score),
            fmt_opt(event.delivery_pct),
            event.notes
        ));
    }
    lines.join("\n")
}

fn fmt_opt(value: Option<f64>) -> String {
    value
        .filter(|v| v.is_finite())
        .map(|v| format!("{v:.2}"))
        .unwrap_or_else(|| "-".to_string())
}

fn fmt_nan(value: f64) -> String {
    if value.is_finite() {
        format!("{value:.2}")
    } else {
        "-".to_string()
    }
}

fn build_backtest_config(
    as_of: NaiveDate,
    args: CommonBacktestArgs,
    cli_cfg: &CliConfig,
) -> anyhow::Result<BacktestConfig> {
    let (entry_expr, exit_expr) = resolve_strategy_exprs(
        args.strategy_name.as_deref(),
        args.entry_expr,
        args.exit_expr,
        cli_cfg,
    )?;
    let tickers = args.tickers.map(|raw| {
        raw.split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    });
    Ok(BacktestConfig {
        market: args.market.clone(),
        as_of,
        hold: args.hold,
        top: args.top,
        entry_expr,
        exit_expr,
        stop_loss: args.stop_loss,
        take_profit: args.take_profit,
        trailing_stop: args.trailing_stop,
        slippage_bps: args.slippage_bps,
        commission_bps: args.commission_bps,
        initial_capital: args.initial_capital,
        benchmark: args.benchmark.unwrap_or_else(|| {
            if args.market == "india" {
                "^NSEI"
            } else {
                "SPY"
            }
            .to_string()
        }),
        strategy_name: args.strategy_name,
        tickers,
        universe_file: args.universe_file,
        max_universe: args.max_universe,
        min_price: normalize_optional_filter(args.min_price),
        min_avg_dollar_volume: normalize_optional_filter(args.min_avg_dollar_volume),
        avg_dollar_volume_window: args.adv_window,
        reserve_multiple: args.reserve_multiple,
        reinvest: !args.no_reinvest,
        slippage_model: parse_slippage_model(
            &args.slippage_model,
            args.slippage_bps,
            args.half_spread_bps,
            args.vol_impact_k,
        )?,
        gap_fills: !args.no_gap_fills,
        entry_order_type: parse_entry_order(&args.entry_order)?,
        entry_limit_bps: args.entry_limit_bps,
        allow_reentry: args.allow_reentry,
        max_reentries: args.max_reentries,
        partial_exits: parse_partial_exits(&args.partial_exit_args)?,
        price_adjustment: parse_price_adjustment(&args.price_adjustment)?,
    })
}

fn resolve_strategy_exprs(
    strategy_name: Option<&str>,
    entry_expr: Option<String>,
    exit_expr: Option<String>,
    cli_cfg: &CliConfig,
) -> anyhow::Result<(String, Option<String>)> {
    if let Some(name) = strategy_name {
        let alias = cli_cfg
            .strategies
            .get(name)
            .cloned()
            .or_else(|| built_in_strategy(name))
            .with_context(|| format!("unknown strategy alias {name:?}"))?;
        return Ok((entry_expr.unwrap_or(alias.entry), exit_expr.or(alias.exit)));
    }
    let entry_expr = entry_expr.context("--entry (or --strategy) is required")?;
    Ok((entry_expr, exit_expr))
}

fn normalize_optional_filter(value: Option<f64>) -> Option<f64> {
    match value {
        Some(0.0) => None,
        other => other,
    }
}

fn parse_slippage_model(
    name: &str,
    slippage_bps: f64,
    half_spread_bps: f64,
    vol_impact_k: f64,
) -> anyhow::Result<SlippageModel> {
    match name {
        "fixed" => Ok(SlippageModel::Fixed { bps: slippage_bps }),
        "half-spread" => Ok(SlippageModel::HalfSpread { half_spread_bps }),
        "vol-impact" => Ok(SlippageModel::VolumeImpact { k: vol_impact_k }),
        "composite" => Ok(SlippageModel::Composite {
            fixed_bps: slippage_bps,
            half_spread_bps,
            vol_impact_k,
        }),
        _ => anyhow::bail!("unknown --slippage-model {name:?}"),
    }
}

fn parse_entry_order(raw: &str) -> anyhow::Result<EntryOrderType> {
    match raw {
        "moo" => Ok(EntryOrderType::Moo),
        "moc" => Ok(EntryOrderType::Moc),
        "limit" => Ok(EntryOrderType::Limit),
        _ => anyhow::bail!("unknown --entry-order {raw:?}"),
    }
}

fn parse_price_adjustment(raw: &str) -> anyhow::Result<PriceAdjustment> {
    match raw {
        "full" => Ok(PriceAdjustment::Full),
        "splits_only" => Ok(PriceAdjustment::SplitsOnly),
        "none" => Ok(PriceAdjustment::None),
        _ => anyhow::bail!("unknown --price-adjustment {raw:?}"),
    }
}

fn parse_partial_exits(raw: &[String]) -> anyhow::Result<Vec<(f64, f64)>> {
    raw.iter()
        .map(|item| {
            let (profit, shares) = item.split_once(':').with_context(|| {
                format!("--partial-exit expects PROFIT_FRAC:SHARES_FRAC, got {item:?}")
            })?;
            Ok((profit.parse()?, shares.parse()?))
        })
        .collect()
}

fn print_backtest_result(result: &screener_rs::backtester::models::BacktestResult) {
    println!("trades,{}", result.trades.len());
    for (key, value) in &result.metrics {
        println!("{key},{value}");
    }
}

fn print_trade_csv(result: &screener_rs::backtester::models::BacktestResult) {
    println!(
        "ticker,rank,signal_date,entry_date,entry_price,exit_date,exit_price,exit_reason,shares,entry_cost,exit_value,pnl,return_pct,dividend_income"
    );
    for trade in &result.trades {
        println!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            trade.ticker,
            trade.rank,
            trade.signal_date,
            trade.entry_date,
            trade.entry_price,
            trade.exit_date,
            trade.exit_price,
            trade.exit_reason,
            trade.shares,
            trade.entry_cost,
            trade.exit_value,
            trade.pnl,
            trade.return_pct,
            trade.dividend_income
        );
    }
}

fn print_screen_rows(rows: &[screener_rs::screeners::models::ScreenRow]) {
    println!("rows,{}", rows.len());
    for row in rows {
        let symbol = row.name.as_deref().or(row.ticker.as_deref()).unwrap_or("-");
        let close = row
            .numeric("close")
            .map(|value| format!("{value:.2}"))
            .unwrap_or_else(|| "-".to_string());
        let score = row
            .numeric("setup_score")
            .or_else(|| row.numeric("garp_score"))
            .map(|value| format!("{value:.2}"))
            .unwrap_or_else(|| "-".to_string());
        println!("{symbol},{close},{score}");
    }
}
