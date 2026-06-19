# Quantitative Audit — Screening & Backtesting Platform

**Audit date:** 2026-06-19
**Repository:** `screener` (US + Indian equities; screeners, indicators, event-driven & vectorized backtesters, optimization, reporting)
**Scope:** Full quantitative correctness audit — every screener, indicator, signal generator, execution simulator, portfolio-accounting module, performance/risk metric, and the data-ingestion layer.
**Method:** Evidence over implementation. Each subsystem was read line-by-line, then independently re-computed on hand-crafted synthetic inputs with known analytical answers, cross-checked against `empyrical-reloaded` / `pandas`-reference implementations, and probed for look-ahead via future-bar perturbation. Existing tests were run **and** independently judged for whether they assert correctness or merely pin current behavior.

**Baseline test suite:** `652 passed, 20 skipped` (clean). With the optional `vectorbt` extra installed, an additional 34 cross-engine/vbt tests pass (they **skip silently** without it — see I-9).

---

## 1. Production-readiness verdict

**The platform is SAFE for production use ONLY in its default configuration, and ONLY for relative/price-return strategy research — NOT for total-return claims, point-in-time historical screening, or absolute-return reporting without the fixes below.**

The core event-driven engine, indicators, signal generation, and the default-mode (`price_adjustment="full"`) price path are **financially sound and free of look-ahead bias** — this was the single most important thing to establish, and it holds up under independent verification. However, **three HIGH-severity systemic biases** make several advertised features unsafe as-is:

1. **Survivorship bias is baked into every default backtest** (universe = today's index members, regardless of the backtest date).
2. **The opt-in "correct total-return" adjustment modes (`splits_only`/`none`) are broken** — splits are not applied and dividends never reach the equity curve.
3. **Historical screening tools that accept `--as-of` leak future fundamentals** (conviction screener; India earnings backtest).

A quant researcher can trust **price-return, single-name and explicit-point-in-time-universe backtests in default mode**. They must **not** trust default-universe historical backtests (survivorship), `splits_only`/`none` results (corporate actions), or `--as-of` historical screens (leakage) until the HIGH findings are fixed.

---

## 2. Component inventory & confidence

| # | Component | Files | Verdict | Confidence |
|---|-----------|-------|---------|-----------|
| Indicators | SMA, EMA, RMA/Wilder, RSI, ATR, Bollinger, stdev, Supertrend | `indicators/` | **VERIFIED** (1 MED warmup bug in RSI) | High |
| Metrics | total return, CAGR, vol, Sharpe, max DD, Calmar, beta, PSR/DSR | `backtester/metrics.py` | **VERIFIED** | High |
| Metrics — Sortino & alpha | downside dev / alpha annualization | `backtester/metrics.py` | **PARTIALLY** (non-standard, inflated) | Medium |
| Execution engine | timing, sizing, cash, costs, slippage, accounting identity, look-ahead | `backtester/{core,engine,fills,slippage,day_loop,portfolio}.py` | **VERIFIED** | High |
| Corporate actions | split/dividend handling in `splits_only`/`none` | `backtester/{data,portfolio,core}.py` | **UNVERIFIED → BROKEN** | — |
| Strategy / signal generation | DSL parser, crossover, pine ports, registry | `strategies/`, `backtester/pine.py` | **VERIFIED** (no look-ahead) | High |
| Vectorized engine (vbt) | bar-shift, cross-engine reconciliation | `backtester/vbt_sweep.py` | **VERIFIED** | High (when extra installed) |
| Walk-forward / rolling | split boundaries, lookback | `backtester/{rolling,vbt_sweep}.py`, `optimization/` | **VERIFIED** | High |
| Optimization reporting | grid search IS/OOS labeling, Monte-Carlo | `backtester/optimization/` | **PARTIALLY** (selection-bias not disclosed) | Medium |
| Earnings/PEAD backtest | date alignment | `earnings_backtest/` | **PARTIALLY** (India period-end leak) | Medium |
| Screener: GARP | growth/value composite | `garp.py`, `commands/garp.py` | **VERIFIED** (1 LOW latent) | High |
| Screener: RS-breakout | RS + Supertrend + breakout | `commands/rs_breakout.py` | **VERIFIED** (clean PIT) | High |
| Screener: Unusual-volume | RVOL / z-score / percentile | `unusual_volume/` | **VERIFIED** (clean PIT) | High |
| Screener: Institutional / Insider / Pledge | 13F, Form-4, pledge | `institutional.py`, `pledge.py`, `commands/insiders.py` | **VERIFIED** (1 LOW) | High |
| Screener: Conviction | 6-pillar composite | `conviction.py`, `commands/conviction.py` | **PARTIALLY** (`--as-of` leak) | Medium |
| Data layer | survivorship, PIT, tz, cache | `universes.py`, `backtester/data.py`, `providers.py` | **PARTIALLY** (HIGH survivorship) | Medium |

---

## 3. Findings — severity-ranked

### HIGH

**H-1 — Survivorship bias: `as_of` does not produce a point-in-time universe.**
`universes.py:89-107` `load_current_universe(name, as_of=…)` uses `as_of` **only as a cache filename**; on a miss it scrapes the *current* Wikipedia S&P 500 / Nifty table via `_fetch_sp500()` (no date). A 2018 backtest silently **excludes every company delisted/removed 2018→2026** and **includes** post-2018 IPOs.
*Evidence:* `_fetch_sp500()` called with no date argument; `grep` confirms `as_of` flows only to `_read_cache`/`_write_cache`.
*Mitigation that exists:* `--point-in-time` (sp500 only) suppresses entries before each name's "Date added" (`rolling.py:86-89`) — but it is **one-sided** (no removal masking, removed/delisted names never reconstructed).
*Fix:* Build a real PIT membership table (added **and** removed dates, incl. delisted symbols) and have `as_of` filter on it; or remove/rename the misleading `as_of` param and **warn** when `as_of < today` with the live scrape.

**H-2 — `split_factor` is computed but never applied (`splits_only`/`none` modes).**
`backtester/data.py:221` writes a `split_factor` column; **no code anywhere reads it** (`grep split_factor` → only the producer line + a column-name list). In `splits_only`/`none`, raw unadjusted OHLC flows into both signals and fills, so a real 2:1 split reads as a **−50% return**.
*Evidence:* Synthetic 2:1 split → engine books `pnl=-50000` (−50%) vs expected ≈0. Confirmed independently by two agents + direct grep.
*Impact:* Any `splits_only`/`none` backtest spanning a split is catastrophically wrong. The mode's help text (`historical.py:548`) markets it as the split-adjusting/total-return path — false.
*Fix:* Apply `split_factor` to OHLC (and divide volume) before signal/fill evaluation when not in `full` mode; add a regression test asserting equity ≈ constant across a flat-price split.

**H-3 — Dividend cash is dropped from the equity curve (`splits_only`/`none`).**
`core.py:187` credits dividends to the live `Portfolio._cash` (`credit_dividends`), but `portfolio.py:248 build_equity_curve` rebuilds the returned equity curve from `entry_cost`/`exit_value`/MTM only and **never references `dividend_income`**.
*Evidence:* Flat price 100, \$2 dividend, 1000 shares → final equity `100000.0` (the \$2000 dividend vanished). Dividend *does* appear in the per-trade `Trade.dividend_income` field but not in the equity curve / metrics.
*Impact:* Total return, CAGR, Sharpe all understated by the full dividend stream — contradicting the mode's stated purpose ("credit dividends as cash").
*Fix:* Thread `dividend_income` into `build_equity_curve` (credit to cash on/after ex-date).

**H-4 — Conviction screener `--as-of` leaks future fundamentals/insider/pledge.**
`commands/conviction.py:56` accepts `--as-of` and labels the card "as of <date>". `as_of` correctly flows into price pillars (trend/breakout/volume) but the **smart-money, fundamentals, and risk pillars fetch *latest* data** (`conviction.py:574-581`, `_load_fundamentals`, `_extract_promoter_pct` takes `df[-1]`, pledge = latest filing).
*Impact:* A historical conviction card mixes point-in-time prices with today's fundamentals/holdings/pledge — classic look-ahead that inflates historical conviction.
*Fix:* Thread `as_of` into the fundamental/insider/pledge loaders (select statement/filing with report date ≤ `as_of`), or skip those pillars when `as_of` is materially in the past.

**H-5 — India earnings backtest keys events on fiscal period-end, not filing date.**
`earnings_backtest/data.py:380-395` sets `earnings_date` to the period-end (`"Mar 2024"`→`2024-03-31`), but Indian Q4 results are announced ~45–60 days later. Any event study applies the (then-unknown) result ~6 weeks early — look-ahead.
*Fix:* Use the actual NSE announcement date (`fetch_earnings_dates_nse` already has `sort_date` at `data.py:276`) or add a configurable filing lag.

### MEDIUM

**M-1 — FMP raw path emits no split/dividend columns → provider-dependent adjustment.**
`backtester/data.py:430-467`: in `splits_only`/`none`, tickers served by the **FMP fallback** get raw close with no split/dividend columns, while yfinance-served tickers in the same run are adjusted — silent inconsistency (a split looks like −50% only for FMP names). *Fix:* reconstruct factor from `adjClose/close` breaks, or disable FMP fallback when `auto_adjust=False`.

**M-2 — Sortino downside-deviation denominator is non-standard → Sortino systematically inflated.**
`metrics.py:99-102` uses `excess[excess<0].std(ddof=0)` — pandas demeans against the negative subset's own mean and divides by *k* (count of down days), not *N*, vs the canonical target-downside-deviation (RMS of below-target returns over N). Example `[.05,.05,.05,-.01,-.03]`: code denom `0.0100` vs standard `0.0141` → Sortino `34.9` vs `24.7` (~√(N/k) too high); returns `0` whenever <2 distinct negatives. *Note:* tests pin this as an "intended design choice" — but it is not financially standard. *Fix:* `sqrt(mean(minimum(excess,0)**2))`.

**M-3 — RSI returns 100 during warmup instead of NaN.**
`indicators/plugins/rsi.py:18-21`: during the first n−1 bars `rma_dn` is NaN, `NaN>0` is False → `rs=inf` → RSI=100 (spurious "overbought"). Inconsistent with RMA/ATR/SMA/stdev (which return NaN). *Fix:* `out[np.isnan(rma_up)] = np.nan`. (Two edge tests pin the buggy behavior and must be updated.)

**M-4 — `optimize grid` reports in-sample, selection-biased metrics with no caveat.**
`optimization/reporting.py` `print_grid_table` prints the best-of-grid in-sample Sharpe as the headline with no OOS split and no disclaimer (unlike `vbt_sweep`, which carries one). *Fix:* add an in-sample/selection-bias banner; steer users to `optimize walk-forward`.

**M-5 — Alpha annualized arithmetically (`intercept*252`) not geometrically** (`metrics.py:75-77`). Overstates alpha; tests pin it. *Fix:* `(1+intercept)**252 - 1`.

### LOW

- **L-1** `_has_range` ±3-day cache tolerance (`data.py:255`) leaves up to 3 trailing sessions stale on live/recent runs.
- **L-2** Universe/membership file cache has no TTL (`universes.py`) — once fetched, never refreshes despite reconstitution.
- **L-3** GARP `inv_peg = 1 - peg.rank(pct=True)` (`garp.py:151`) rewards negative (loss-making) PEG with the top value-score; masked in the shipped screen by the `_passes_garp` gate, latent if `add_garp_score` is reused on ungated data. *Fix:* `peg = peg.where(peg>0)` before ranking.
- **L-4** Institutional `qoq_change_pct` denominator includes changeless holders (`institutional.py:72-74`) — secondary field only.
- **L-5** Monte-Carlo uses IID resample-with-replacement (`optimization/monte_carlo.py:72`), discarding trade-sequence autocorrelation → optimistic ruin/drawdown; not disclosed.
- **L-6** Vol/Sharpe/Sortino use population std (ddof=0) vs industry sample std (ddof=1) — known √(N/(N-1)) factor, negligible at N=252, material for short series.
- **L-7** WF "overfit ratio" (train/test) and "efficiency" (oos/is) use opposite orientations across the two WF implementations — confusing, not wrong.
- **L-8** Numpy-plugin EMA has no NaN warmup (seeds `out[0]=x[0]`) vs DSL EMA's `min_periods` — biased early bars (causal, converges after warmup).

### PROCESS / COVERAGE

- **I-9** Cross-engine reconciliation and all vbt tests **skip silently** when the optional `vectorbt` extra is absent (the default venv state) — CI can be green while the secondary engine is never validated. *Fix:* a CI lane with the extra, or `xfail(strict=True)`.
- Coverage gaps that let the HIGH bugs pass CI: no test exercises a split end-to-end, asserts dividends reach the equity curve, asserts default-universe survivorship, or asserts `--as-of` excludes future fundamentals.

---

## 4. What is independently VERIFIED correct (trust these)

- **Execution timing / no look-ahead:** signal from bar *t*'s close fills at *t+1* open/close (`fills.py:43-70`); future-bar perturbation (×1000) leaves entry date/price bit-identical. Exit loop starts at `entry_idx+1` (no same-bar round-trip).
- **Accounting identity:** `cash + Σ(shares·close) == equity` holds exactly pre-entry, mid-trade, and post-close on synthetic scenarios. Cash signs, commission (bps on notional both sides), and slippage (always adverse) verified to ~1e-9.
- **Metrics:** total return, CAGR (geometric, correct year-fraction — the prior CAGR off-by-one is **fixed**), vol, Sharpe (rf de-annualized correctly), max drawdown (correct sign, on equity curve), Calmar, beta (OLS), PSR/DSR (López de Prado) all match hand-math and `empyrical` to ≤1e-9.
- **Indicators:** SMA, EMA, Wilder RMA, ATR, Bollinger, stdev, Supertrend match independent numpy references to ≤1e-12; **RSI/ATR correctly use Wilder smoothing** (α=1/n), EMA uses α=2/(n+1). No look-ahead in any indicator.
- **Signal generation:** crossover/crossunder compare *t* vs *t−1* (never *t+1*); recursive-descent DSL parser (no `eval`); all 7 callable plugins pass future-perturbation. Pine next-bar-open convention preserved.
- **Vectorized engine:** entries/exits `.shift(1)` then fill at next open; reconciles with the event engine to rtol=1e-9 on single-name trades; multi-ticker divergence is bounded and explained (cash-sharing).
- **Walk-forward / rolling:** train/test splits strictly non-overlapping, test strictly after train, winner chosen on train and applied to test; rolling lookback uses `searchsorted(side="right")-1` (never a future bar).
- **Screeners GARP / RS-breakout / Unusual-volume:** factor math, ranking direction, and (for RS-breakout & unusual-volume) point-in-time correctness all verified — baselines exclude the current bar (`.shift(1)`), bars clipped to `as_of`.
- **Timezone:** US (ET) / India (IST) daily bars normalize to correct local session date; no day-shift.

---

## 5. Recommendations (priority order)

1. **Fix H-1 (survivorship)** before any default-universe historical backtest is trusted. Until then, document that default backtests are survivorship-biased and require `--point-in-time` or an explicit PIT `--tickers` list.
2. **Fix H-2 / H-3 / M-1 (corporate actions)** or **disable the `splits_only`/`none` modes** — they currently produce wrong results while being marketed as the "correct" total-return path. Default `full` mode is fine.
3. **Fix H-4 / H-5 (PIT leakage)** in the conviction screener and India earnings backtest, or block `--as-of` < today for the leaking pillars.
4. **Decide on M-2 Sortino** — either adopt the standard target-downside-deviation or prominently label the metric as non-standard in output (a researcher comparing Sortino across platforms will be misled).
5. Add the missing regression tests (split round-trip, dividend-in-equity-curve, default-universe survivorship, `--as-of` fundamental exclusion) and a `vectorbt`-enabled CI lane (I-9).
6. Address MEDIUM/LOW items as quality hardening.

---

## 6. Confidence summary

| Use case | Verdict |
|----------|---------|
| Price-return backtest, single name or explicit PIT universe, **default** adjustment | **TRUSTWORTHY** |
| Default-universe historical backtest | **NOT trustworthy** (survivorship — H-1) |
| `splits_only` / `none` total-return backtest | **NOT trustworthy** (H-2, H-3, M-1) |
| Historical screening via `--as-of` (conviction) | **NOT trustworthy** (leakage — H-4) |
| India earnings/PEAD event study | **Caution** (period-end leak — H-5; PEAD path itself is clean) |
| Indicators, signal generation, metrics (ex-Sortino), execution mechanics | **TRUSTWORTHY** |
| Sortino / arithmetic-alpha values | **Use with caution** (non-standard — M-2, M-5) |
| Walk-forward validation; `optimize grid` headline numbers | WF **trustworthy**; grid numbers are **in-sample/selection-biased** (M-4) |

*No repository source was modified during this audit. All verification used synthetic scripts under `/tmp`. The only environment change was installing the optional `vectorbt` extra into `.venv` to exercise the gated reconciliation tests.*
