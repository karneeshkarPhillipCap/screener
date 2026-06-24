"""Ultimate Momentum Strategy."""

from __future__ import annotations

import pandas as pd
import numpy as np

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_ultimate_momentum(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    benchmark_bars = ctx.price_panel.get(ctx.cfg.benchmark, pd.DataFrame())
    if benchmark_bars is None or benchmark_bars.empty:
        return ctx.bars_by_tv

    benchmark_close = benchmark_bars["close"].astype(float)
    bench_sma100 = benchmark_close.rolling(100, min_periods=100).mean()
    bench_regime = benchmark_close > bench_sma100

    N = 90
    x = np.arange(N)
    x_mean = x.mean()
    x_diff = x - x_mean
    var_x = np.var(x)
    sum_x_diff_sq = np.sum(x_diff**2)

    prepared = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue

        df = bars.copy().sort_index()
        close = df["close"].astype(float)

        # 1. Clenow Quality Momentum (90-day)
        y = np.log(close)
        clenow_score = np.zeros(len(y))
        if len(y) >= N:
            w = x_diff[::-1]
            conv = np.convolve(y, w, mode="valid")
            slope = np.full(len(y), np.nan)
            slope[N - 1 :] = conv / sum_x_diff_sq
            ann_slope = (np.exp(slope) ** 252) - 1.0

            cov = np.full(len(y), np.nan)
            cov[N - 1 :] = conv / N
            var_y = y.rolling(N).var(ddof=0)
            var_y_safe = np.where(var_y == 0, np.nan, var_y)
            r2 = (cov**2) / (var_x * var_y_safe)
            clenow_score = np.nan_to_num(ann_slope * r2)

        # 2. Accelerating Dual Momentum (3m, 6m, 12m)
        roc_63 = close / close.shift(63) - 1.0
        roc_126 = close / close.shift(126) - 1.0
        roc_252 = close / close.shift(252) - 1.0
        adm_score = (roc_63 + roc_126 + roc_252) / 3.0
        adm_score = np.nan_to_num(adm_score)

        # 3. Volatility-Adjusted Momentum (90-day)
        roc_90 = close / close.shift(90) - 1.0
        daily_returns = close.pct_change()
        vol_90 = daily_returns.rolling(90).std() * np.sqrt(252)
        vol_90_safe = np.where(vol_90 == 0, np.nan, vol_90)
        vol_adj_score = np.nan_to_num(roc_90 / vol_90_safe)

        # Combined Score (Geometric-like product)
        # To ensure we don't multiply negatives into positives, we clip at 0
        clenow_clipped = np.clip(clenow_score, 0, None)
        adm_clipped = np.clip(adm_score, 0, None)
        vol_adj_clipped = np.clip(vol_adj_score, 0, None)

        ultimate_score = clenow_clipped * adm_clipped * vol_adj_clipped

        # Filters
        regime = bench_regime.reindex(df.index).fillna(False)
        stock_regime = close > close.rolling(200, min_periods=200).mean()

        df["ultimate_score"] = np.where(
            regime & stock_regime & (ultimate_score > 0), ultimate_score, 0.0
        )
        prepared[symbol] = df

    return prepared


def _ult_lookback() -> int:
    return 252


@strategy(
    "ultimate_momentum",
    entry="ultimate_score > 0.001",
    exit="ultimate_score <= 0",
    prepare_bars=_prepare_ultimate_momentum,
    required_lookback=_ult_lookback,
)
def _ultimate_momentum() -> None:
    """Expression-only strategy. Body unused."""
