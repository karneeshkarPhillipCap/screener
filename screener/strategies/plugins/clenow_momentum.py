"""Clenow's Quality Momentum Strategy."""

from __future__ import annotations

import pandas as pd
import numpy as np

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_clenow(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    benchmark_bars = ctx.price_panel.get(ctx.cfg.benchmark, pd.DataFrame())
    if benchmark_bars is None or benchmark_bars.empty:
        return ctx.bars_by_tv

    benchmark_close = benchmark_bars["close"].astype(float)
    bench_sma = benchmark_close.rolling(100, min_periods=100).mean()
    bench_regime = benchmark_close > bench_sma

    N = 90
    x = np.arange(N)
    x_mean = x.mean()
    x_diff = x - x_mean
    sum_x_diff_sq = np.sum(x_diff**2)
    var_x = np.var(x)

    prepared: dict[str, pd.DataFrame] = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue

        df = bars.copy().sort_index()
        close = df["close"].astype(float)

        y = pd.Series(np.log(close.to_numpy()), index=close.index)

        if len(y) < N:
            df["clenow_score"] = 0.0
            prepared[symbol] = df
            continue

        # Calculate rolling slope using convolution
        w = x_diff[::-1]
        conv = np.convolve(y, w, mode="valid")

        slope = np.full(len(y), np.nan)
        slope[N - 1 :] = conv / sum_x_diff_sq

        # Annualized slope
        ann_slope = (np.exp(slope) ** 252) - 1.0

        # Calculate R^2
        cov = np.full(len(y), np.nan)
        cov[N - 1 :] = conv / N

        var_y = y.rolling(N).var(ddof=0)

        # To avoid division by zero
        var_y_safe = pd.Series(np.where(var_y == 0, np.nan, var_y), index=df.index)
        r2 = (cov**2) / (var_x * var_y_safe)

        clenow_score = ann_slope * r2

        # Align benchmark regime
        regime = bench_regime.reindex(df.index).fillna(False)

        # Entry score: clenow_score if positive and benchmark is in bull regime
        df["clenow_score"] = np.where(regime & (clenow_score > 0), clenow_score, 0.0)
        prepared[symbol] = df

    return prepared


def _clenow_lookback() -> int:
    return 200


@strategy(
    "clenow_momentum",
    entry="clenow_score > 0.05",
    exit="clenow_score <= 0",
    prepare_bars=_prepare_clenow,
    required_lookback=_clenow_lookback,
)
def _clenow_momentum() -> None:
    """Expression-only strategy. Body unused."""
