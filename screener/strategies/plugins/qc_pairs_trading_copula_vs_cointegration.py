"""Pairs Trading Copula Vs Cointegration."""

from __future__ import annotations

import pandas as pd
import numpy as np

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_pairs(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    # Cointegration approx for GLD/DGL or arbitrary pair
    # We will just calculate Z-score of spread against SPY for any asset as an approximation
    # If the user passes GLD and DGL, they will be evaluated against SPY or a benchmark

    benchmark_bars = ctx.price_panel.get(ctx.cfg.benchmark, pd.DataFrame())
    if benchmark_bars is None or benchmark_bars.empty:
        return ctx.bars_by_tv

    bench_log = np.log(benchmark_bars["close"].astype(float))

    prepared = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue

        df = bars.copy().sort_index()
        asset_log = np.log(df["close"].astype(float))

        # Rolling Beta and Alpha over 252 days
        cov = asset_log.rolling(252).cov(bench_log)
        var = bench_log.rolling(252).var()
        beta = cov / var
        alpha = asset_log.rolling(252).mean() - beta * bench_log.rolling(252).mean()

        spread = asset_log - (beta * bench_log + alpha)
        zscore = (spread - spread.rolling(252).mean()) / spread.rolling(252).std()

        # If zscore < -1.0, spread is too low, buy asset
        df["qc_pairs_zscore"] = zscore
        prepared[symbol] = df

    return prepared


def _lookback() -> int:
    return 504


@strategy(
    "qc_pairs_trading_copula_vs_cointegration",
    entry="qc_pairs_zscore < -1.0",
    exit="qc_pairs_zscore > 0.0",
    prepare_bars=_prepare_pairs,
    required_lookback=_lookback,
)
def _qc_pairs() -> None:
    pass
