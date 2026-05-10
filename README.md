## Screener

Stock screening, backtesting, and research CLI for US and Indian markets.

Run commands through `uv`:

```bash
uv run screener --help
uv run screener screen -m india -n 30
```

The repo also has `just` shortcuts that use the local virtualenv:

```bash
just --list
just screen -m us -n 20 --csv
just backtest -m us --as-of 2026-03-20 --entry "close > 0" --tickers AAPL,MSFT
```

## Commands

### `screen`

TradingView-based technical screener.

```bash
uv run screener screen -m us -c ema -n 50
uv run screener screen -m india -c ema -c breakout --detail
uv run screener screen -m us -c intraday_momentum --csv
```

Features:

- Markets: `us`, `india`.
- Criteria: `ema`, `breakout`, `ema_breakout`, `value`, `quality`, `cheap_quality`, `undervalued`, `dividend`, `momentum_value`, `intraday_momentum`, `intraday_breakout`.
- Local `setup_score` ranking by default.
- Optional CSV output with `--csv`.
- Optional fundamentals with `--detail`.
- TradingView cache controls with `--cache-ttl` and `--refresh`.
- Saves non-CSV runs to `~/.screener/history.db` and prints added/removed tickers versus the previous run.

### `garp`

Finds GARP stocks using market-specific fundamental data.

```bash
uv run screener garp -m india -n 30
uv run screener garp -m us --universe-size 300 --workers 8 --csv
just garp -m india -n 30
```

### `promoter-buys`

Finds stocks where promoter or insider holdings increased.

```bash
uv run screener promoter-buys -m india --min-change 0.5
uv run screener promoter-buys -m us --min-yf-net-pct 0.01
just promoter-buys -m india --min-change 0.5
```

India mode uses screener.in promoter data with optional yfinance cross-checks. US mode uses yfinance insider transaction data.

### `rs-breakout`

Screens for relative strength, SuperTrend, breakout, and volume setups.

```bash
uv run screener rs-breakout -m india -n 50
uv run screener rs-breakout -m us --tickers AAPL,MSFT,NVDA --no-output-files
uv run screener rs-breakout -m india --json rs.json --md rs.md
just rs-breakout -m india -n 50
```

### `unusual-volume`

Detects abnormal trading volume across a market or a ticker list.

```bash
uv run screener unusual-volume -m us --tickers AAPL,MSFT
just unusual-volume -m india
```

### `operator-scan`

NSE Operator Intent screener. It combines NSE Cash Bhavcopy delivery/VWAP data with F&O open interest changes, labels operator action, and writes a CSV.

```bash
uv run screener operator-scan
uv run screener operator-scan --date 2026-05-08 --only-actions --verbose
uv run screener operator-scan --universe fo --output operator.csv
just operator-scan --only-actions
```

Action labels include Long Build-up, Short Covering, Short Build-up, Long Unwinding, and High_Momentum_Watch.

## Backtesting

### `backtest-historical`

Runs a historical point-in-time backtest. This is wrapped by `just backtest`.

```bash
uv run screener backtest-historical -m us --as-of 2026-03-20 --entry "close > 0" --tickers AAPL,MSFT --hold 5 --top 2
just backtest -m india --as-of 2026-03-20 --entry "close > 0" --tickers RELIANCE,TCS --hold 5 --top 2
```

### `backtest-rolling`

Runs a daily rolling backtest across a date window.

```bash
uv run screener backtest-rolling -m us --years 2 --strategy rs_breakout --top 10
uv run screener backtest-rolling -m india --start 2024-01-01 --end 2026-05-08 --entry "close > sma(close, 20)" --exit false
just backtest-rolling -m us --years 2 --strategy rs_breakout --top 10
```

Supports position sizing slots, holding period, stop loss, take profit, trailing stop, slippage/commission, benchmark, liquidity filters, custom tickers, CSV ledger output, and optional dashboard output.

### `backtest-lab`

Launches a local browser UI for comparing rolling backtest strategies.

```bash
uv run screener backtest-lab
uv run screener backtest-lab --host 127.0.0.1 --port 8766
just backtest-lab
```

### Standalone Pine Runner

The standalone Pine strategy runner is not a `uv run screener` subcommand; it is a separate script wrapped by `just pine`.

```bash
just pine --market us --years 3 --limit 50
just pine-india --years 2
uv run python run_pinescript_strategies.py --market us --years 3 --limit 50
```

## Optimization

### `optimize grid`

Runs exhaustive grid search over backtest parameter ranges.

```bash
uv run screener optimize grid -m us --years 2 --strategy rs_breakout --stop-loss 0.05,0.08 --take-profit 0.1,0.15 --hold 5,10
just optimize grid -m us --years 2 --strategy rs_breakout --stop-loss 0.05,0.08 --take-profit 0.1,0.15 --hold 5,10
```

### `optimize walk-forward`

Runs rolling train/test walk-forward optimization.

```bash
uv run screener optimize walk-forward -m india --years 3 --strategy rs_breakout --train-days 252 --test-days 63
just optimize walk-forward -m india --years 3 --strategy rs_breakout --train-days 252 --test-days 63
```

### `optimize validate`

Runs Monte Carlo stress testing on an existing trade ledger.

```bash
uv run screener optimize validate --trades trades.csv --iterations 5000 --json validation.json
just optimize validate --trades trades.csv --iterations 5000 --json validation.json
```

## Utility Commands

### `usage-report`

Shows successful feature usage counts from Turso.

```bash
uv run screener usage-report
just usage-report
```

## Config File

The CLI can load YAML or JSON defaults with `--config`. The repo includes an example at `screener.yaml`.

```bash
uv run screener --config screener.yaml screen
uv run screener --config screener.yaml backtest-historical
uv run screener --config screener.yaml optimize grid
```

Config files must contain a top-level mapping. Top-level keys are global options and command names. For nested Click command groups, such as `optimize`, put the subcommand under the group name.

```yaml
log_level: INFO
log_json: false

screen:
  market: india
  criteria_names:
    - ema
    - breakout
  limit: 30
  order_by: setup_score
  cache_ttl: 15m

backtest-historical:
  market: us
  as_of: "2026-03-20"
  tickers: AAPL,MSFT,NVDA
  entry_expr: close > sma(close, 20)
  exit_expr: "false"
  hold: 5
  top: 2

unusual-volume:
  market: india
  strength_floor: high
  limit: 50
  buildup_enabled: true

optimize:
  grid:
    market: us
    years: 1
    strategy_name: rs_breakout
    hold: 5,10,20
    top: 10
    metric: sharpe
```

Use Click parameter names in config, not always the visible flag name. Most are the flag converted to snake case, for example `--cache-ttl` becomes `cache_ttl`. Some commands use custom internal names:

- `--criteria` -> `criteria_names`
- `--sort` -> `order_by`
- `--entry` -> `entry_expr`
- `--exit` -> `exit_expr`
- `--strategy` -> `strategy_name`
- `--csv` -> `output_csv`
- `--strength` -> `strength_floor`
- `--buildup/--no-buildup` -> `buildup_enabled`
- `--json` -> `json_path`
- `--md` -> `md_path`

Explicit CLI flags override values from the config file.

### Global Options

Every `uv run screener ...` command accepts these top-level options before the subcommand:

```bash
uv run screener --config config.yaml screen -m india
uv run screener --log-level DEBUG screen -m us
uv run screener --log-json screen -m us --csv
```

## Just Shortcuts

Current `justfile` recipes:

```bash
just
just help
just help-screen
just help-backtest
just help-backtest-rolling
just help-backtest-lab
just help-garp
just help-promoter-buys
just help-rs-breakout
just help-operator-scan
just help-optimize
just help-pine
just help-unusual-volume
just screen ...
just screen-us ...
just screen-india ...
just backtest ...
just backtest-rolling ...
just backtest-lab ...
just backtest-smoke-us
just backtest-smoke-india
just pine ...
just pine-us ...
just pine-india ...
just unusual-volume ...
just garp ...
just promoter-buys ...
just rs-breakout ...
just operator-scan ...
just optimize ...
just usage-report
just compile
```

All current top-level `uv run screener` commands are wrapped by `just`.

## Price Data Provider

The default price provider is `yfinance` with Financial Modeling Prep fallback when `FMP_API_KEY` is available. Set this environment variable before running a command:

```bash
export FMP_API_KEY="your_fmp_api_key"
```

Then run the project through `uv`, for example:

```bash
uv run screener backtest-historical --tickers AAPL,MSFT --entry "close > sma(close, 20)"
```

FMP responses are cached under `~/.screener/fmp_prices`. Use a command's existing `--refresh` option where available to bypass cached price data.

To force one provider instead of fallback mode, set `SCREENER_PRICE_PROVIDER` to `yfinance` or `fmp`.
