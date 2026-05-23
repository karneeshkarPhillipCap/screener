# Correctness-Verification Findings

Independent verification of the screener/backtester against **external oracles**
(pandas-ta-classic, TA-Lib, empyrical-reloaded, scipy), **hand-derived arithmetic**,
and a **cross-engine reconciliation** (event-driven engine vs vectorbt). Unlike the
existing 234-test suite — which compares the code against its own Pine port and frozen
CSVs — these tests fail only on a *real* discrepancy with a trusted reference.

## How to run

```bash
uv run pytest tests/correctness -q          # offline, 275 tests + 14 live skipped
SCREENER_LIVE_TESTS=1 uv run pytest tests/correctness -q -m network   # opt-in live
uv run mypy && uv run ruff check screener   # quality gates
```

## Suite status

| Gate | Result |
|---|---|
| `tests/correctness` (offline) | **275 passed, 14 skipped** (live, network-gated) |
| Full suite (regression) | **509 passed, 14 skipped** (234 pre-existing + 275 new) |
| mypy | clean (136 source files) |
| ruff (`screener` + `tests/correctness`) | clean |

TA-Lib is present on this machine, so its witness tests run locally; they are gated by
`pytest.importorskip("talib")` and skip cleanly in CI without the C library.

---

## 1. Genuine candidate bug

### CAGR off-by-one — `metrics.py::_cagr`
`years = len(equity) / 252`. A correctly built equity curve has **N+1 points for N
return observations** (`[start, start·(1+r₀), …]`), so the annualization horizon is
always inflated by one bar, which **systematically under-reports CAGR**.

- Screener (253-point equity): `years = 253/252 ≈ 1.004`
- empyrical `cagr(returns)` (252 returns): `years = 252/252 = 1.000`
- Confirmed divergence on a sample equity curve (≈0.28657 vs ≈0.28721); magnitude scales
  with total return and exceeds 1e-5 for any non-trivial curve.

**Recommendation:** use `(len(equity) - 1) / 252`. Verified by
`test_metrics_vs_empyrical.py` and pinned by `test_metrics_golden.py`
(`linspace(100,200,252)` → screener treats it as >1 year by one bar).

---

## 2. Documented design choices (non-standard, not bugs)

These diverge from a textbook/library convention but are internally consistent and
defensible. Each is pinned by a hand golden so a future *unintended* change still fails.

| # | Location | Divergence | Reference | Classification |
|---|---|---|---|---|
| 2.1 | `_sharpe`, `_vol_annual` | population std (ddof=0) | empyrical uses sample std (ddof=1) | **OK** — exact relation `sharpe·√((N-1)/N)=empyrical`, `vol·√(N/(N-1))=empyrical`; verified for N∈{50,126,252,504} |
| 2.2 | `_sortino` | divides by `std(negatives-only, ddof=0)` | empyrical uses RMS of `min(r,0)` over all N | **design choice** — not a scalar factor (1.392 vs 1.150); screener's variant runs larger |
| 2.3 | `_alpha_beta` | `intercept·252` (arithmetic) | empyrical geometric `(1+intercept)^252−1` | **design choice** — daily intercept itself matches scipy to <1e-12; only annualization differs (0.113 vs 0.120) |
| 2.4 | RSI on flat market | `rma_dn==0` → RSI pinned at 100 | n/a | **documented quirk** — a zero-variance series has no downside |
| 2.5 | `data.py::_normalize_frame` | does **not** back-adjust OHLC; only records a `split_factor` column | n/a | **design choice** — back-adjustment is yfinance's `auto_adjust` job or the caller's; factor for `[0,0,2,0,0]`→`[2,2,1,1,1]`, `[0,2,0,3,0]`→`[6,3,3,1,1]` |
| 2.6 | `data.py::tv_to_yf` | `market` arg is ignored when symbol carries an exchange prefix (`NSE:`/`BSE:`) | n/a | **design choice** — prefix wins; `NASDAQ:AAPL`→`AAPL`, `NSE:X`+us→`X.NS` |
| 2.7 | `_obv` (vbt) | cumulative sum starts at 0 | TA-Lib/pandas-ta seed at `volume[0]` | **OK** — differs by a constant; first-differences match to 1e-6 |
| 2.8 | `supertrend_dir` | `direction < 0 == uptrend` | pandas-ta uses `+1 == uptrend` | **OK** — inverted convention; sign agrees after flip on the converged tail |
| 2.9 | `ema` | seeds `out[0]=x[0]` (no SMA warm-up, no NaN) | pandas-ta `presma=False` | **OK** — converges; tail agrees to 1e-6 by ~200 bars for n=20 |
| 2.10 | `garp.py::add_garp_score` | `inv_peg = 1 − peg.rank(pct=True)` is rank-relative | n/a | **design choice** — max possible is `1−1/n`; single-row → 0; best-of-4 row tops out at 92.5, not 100 |

---

## 3. Cross-engine reconciliation (event-driven vs vectorbt)

On the regime where they provably agree (single ticker, 1 slot, SMA crossover, fees=0,
slippage=0, MOO next-open fills, no stops/targets/trailing/partials/dividends, same
300-bar frame):

- **3 trades, identical entry dates and identical entry/exit prices.**
- **`total_return` matches to <1e-10** (0.9172854786751 both).
- Exit dates differ by exactly **1 business day** by construction (event engine exits on
  the signal day at close; vbt shifts the exit signal +1 and fills at next open) — pinned,
  not a bug.
- A multi-ticker control test confirms the engines **do diverge** (>5%) with multiple
  slots (vbt `cash_sharing` vs event-driven slot allocation), so the equality test is
  non-trivial.

**Sharpe gap (~49%, documented, not a bug):** the event engine computes Sharpe over the
active `as_of`-to-last-exit sub-window (~127 traded bars); vbt computes it over the full
300-bar window including idle-cash days with zero return. Different windows → different
annualized Sharpe. The plan's `rtol=5e-2` is **not achievable** without forcing both onto
an identical window; the test instead asserts both are finite, positive, and the gap is
bounded (<100%), and documents the cause.

---

## 4. Verified correct against an independent oracle

These matched a trusted external reference (not the code's own port) within stated tolerance:

- **SMA, STDEV, Bollinger Bands** — exact (1e-9…1e-12) vs pandas-ta-classic *and* TA-Lib;
  all three use population std (ddof=0).
- **EMA / RSI / ATR** — agree with pandas-ta/TA-Lib on the converged tail (1e-6 / 1e-3 / 1e-2).
- **Beta** — matches scipy `linregress` and empyrical to <1e-10.
- **Max drawdown** — matches empyrical to <1e-12.
- **PSR / DSR** (López de Prado) — match an independent scipy witness to <1e-9;
  precondition verified that pandas `.skew()/.kurt()` equal
  `scipy.stats.skew(bias=False)` / `kurtosis(fisher=True, bias=False)`; `_phi`/`_phi_inv`
  bisection agrees with `scipy.stats.norm.cdf/ppf` to <1e-9. Guards confirmed: PSR→0 for
  len<30; DSR with n_trials≤1 reduces to PSR(·,0).
- **Trade mechanics** (hand-derived, event engine) — signal_idx=3 → entry_idx=4 next-open;
  stop/target intrabar fills; gap-down/gap-up fill-at-open vs fill-at-ref under
  `gap_fills`; trailing ratchet; partials via `run_backtest`; time exit; and
  commission+slippage: shares `100000/(100.5·1.001)=994.0308…`, pnl `18568.5956` — all
  match to 1e-6.
- **No lookahead** — `select_candidates`, `simulate_ticker`, `run_backtest`, and the
  rolling engine all produce byte-identical past decisions (dates/prices/selected set)
  when bars strictly after the decision are overwritten with 1000× garbage.
- **Scoring weights** — `_add_setup_score` is exactly `25/30/15/15/10/5/−15`
  (liquidity / trend / momentum / market-cap / rsi-quality / price-quality / overextension);
  `add_garp_score` is exactly `30/20/15/15/10/10`. Component curves verified
  (`rsi_quality` peak at 60; `overextension` ramp 0.12→0.37; `inv_peg` of `[0.5,1,2,4]`→`[0.75,0.5,0.25,0]`).
- **Data layer** — NaN-OHLCV drop (+ cache re-drop), dedupe-by-date keep-last, tz-naive
  index, `tv_to_yf` mapping table, NSE bhavcopy `SERIES=='EQ'` / F&O `FinInstrmTp=='STF'`
  filters, `_parse_bhavcopy_date` dayfirst — all verified offline against pinned inputs.

---

## File map

```
tests/correctness/
  reference_adapters.py              # every reconciliation rule, in one reviewable place
  conftest.py                        # SCREENER_LIVE_TESTS=1 network gating
  test_indicators_vs_reference.py    # pandas-ta + TA-Lib cross-checks (tail/exact)
  test_indicators_golden.py          # hand-derived warm-up seeds + edge contracts
  test_indicators_edge_cases.py      # warm-up NaN, short/flat/single-element, NaN propagation
  test_metrics_vs_empyrical.py       # Sharpe/Vol/CAGR/Sortino/alpha/beta/maxDD vs empyrical
  test_metrics_golden.py             # hand-derived metric goldens
  test_metrics_edge_cases.py         # empty/zero/constant guards
  test_reference_witnesses.py        # scipy witnesses for PSR/DSR/phi/skew/kurt
  fixtures/explicit_bars.py          # deterministic pinned OHLC (no RNG)
  test_hand_computed_trades.py       # 9 hand-computed trade scenarios, two assertion layers
  test_cross_engine_reconciliation.py# event-driven vs vectorbt
  test_lookahead_blindness.py        # T1–T4 future-perturbation invariance
  test_data_layer.py                 # US + India transforms, offline
  test_data_layer_live.py            # @pytest.mark.network live sanity (skipped by default)
  test_scoring.py                    # _add_setup_score + add_garp_score component values
```

The only edit to existing code was registering the `network` / `requires_talib` /
`requires_quantstats` markers and adding the dev dependencies in `pyproject.toml`.
