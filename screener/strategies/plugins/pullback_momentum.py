"""Pullback Momentum Strategy."""

from __future__ import annotations

import pandas as pd
import numpy as np

from screener.strategies.spec import PrepareCtx, strategy
from screener.indicators.plugins.rsi import rsi as _rsi


def _prepare_pullback_momentum(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
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
        y = np.log(close)
        
        # 1. Clenow Momentum Score
        clenow_score = np.zeros(len(y))
        if len(y) >= N:
            w = x_diff[::-1]
            conv = np.convolve(y, w, mode='valid')
            slope = np.full(len(y), np.nan)
            slope[N-1:] = conv / sum_x_diff_sq
            ann_slope = (np.exp(slope) ** 252) - 1.0
            
            cov = np.full(len(y), np.nan)
            cov[N-1:] = conv / N
            var_y = y.rolling(N).var(ddof=0)
            var_y_safe = np.where(var_y == 0, np.nan, var_y)
            r2 = (cov**2) / (var_x * var_y_safe)
            clenow_score = np.nan_to_num(ann_slope * r2)

        # 2. Short-term Pullback (RSI)
        rsi_3 = _rsi(close.to_numpy(), 3)
        
        # Combine
        # Filter regime
        regime = bench_regime.reindex(df.index).fillna(False)
        stock_regime = close > close.rolling(200, min_periods=200).mean()
        
        # We need rsi_3 < 40 to enter. So we only emit a positive score if RSI is low.
        is_pullback = rsi_3 < 40
        
        df["pm_score"] = np.where(
            regime & stock_regime & is_pullback & (clenow_score > 0.05),
            clenow_score,
            0.0
        )
        
        df["rsi_3"] = rsi_3
        prepared[symbol] = df

    return prepared


def _pm_lookback() -> int:
    return 200


@strategy(
    "pullback_momentum",
    entry="pm_score > 0.0",
    exit="rsi_3 > 70",
    prepare_bars=_prepare_pullback_momentum,
    required_lookback=_pm_lookback,
)
def _pullback_momentum() -> None:
    """Expression-only strategy. Body unused."""
