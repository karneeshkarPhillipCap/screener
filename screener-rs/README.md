# screener-rs

Rust migration workspace for the Python `screener` CLI.

Current status:

- Backtester core foundation is implemented as a Rust library.
- `backtest-historical` and `backtest-rolling` use Yahoo chart data by default, with `--prices-csv` still available for offline parity work.
- Live `screen` uses TradingView scanner JSON and filters locally with the Rust criteria engine.
- `promoter-buys`, `rs-breakout`, `unusual-volume`, and `operator-scan` are ported with live provider clones:
  - Yahoo chart API for OHLCV.
  - TradingView scanner JSON for liquid universes.
  - Screener.in HTML shareholding tables for India promoter deltas.
  - FMP Form 4 insider trading for US promoter/insider buys when `FMP_API_KEY` is set.
  - NSE cash/F&O bhavcopy archives for operator scan and India delivery overlays.
- `earnings-backtest`, `vbt-sweep`, `backtest-lab`, `optimize`, and `usage-report` remain registered placeholders.
- YAML config loading supports expression aliases under `strategies:` and `criteria:`.
- JSON config support is intentionally not part of the Rust migration.
- Live GARP fundamental enrichment is not ported yet; `garp` still requires `--input-csv` containing GARP columns.
- Rust unusual-volume currently skips the optional deep India, option-chain, FII/DII, pledge, and buildup overlays.

Price CSV format for the offline fetcher:

```csv
ticker,date,open,high,low,close,volume,adj_close,dividend
AAA,2024-01-02,100,101,99,100.5,100000,,
```

Example:

```bash
cargo run -- backtest-historical \
  --as-of 2024-01-04 \
  --entry "close > sma(close, 3)" \
  --tickers AAA \
  --prices-csv prices.csv
```

The next migration step is broader golden-output comparison for live provider-backed paths, especially NSE delivery overlays and operator scan outputs.
