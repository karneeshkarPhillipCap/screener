"""Hybrid Momentum Strategy."""

from __future__ import annotations

import pandas as pd
import numpy as np

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_hybrid(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
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

    prepared: dict[str, pd.DataFrame] = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue

        df = bars.copy().sort_index()
        close = df["close"].astype(float)

        # Accelerating Returns (3m, 6m, 12m)
        roc_63 = close / close.shift(63) - 1.0
        roc_126 = close / close.shift(126) - 1.0
        roc_252 = close / close.shift(252) - 1.0
        adm_score = (roc_63 + roc_126 + roc_252) / 3.0

        # Clenow R^2 (Quality of trend) over 90 days
        y = pd.Series(np.log(close.to_numpy()), index=close.index)
        r2 = np.zeros(len(y))

        if len(y) >= N:
            w = x_diff[::-1]
            conv = np.convolve(y, w, mode="valid")
            cov = np.full(len(y), np.nan)
            cov[N - 1 :] = conv / N
            var_y = y.rolling(N).var(ddof=0)
            var_y_safe = pd.Series(np.where(var_y == 0, np.nan, var_y), index=df.index)
            r2 = (cov**2) / (var_x * var_y_safe)
            r2 = np.nan_to_num(r2)

        # Volatility penalty (90-day annualized volatility)
        daily_returns = close.pct_change()
        vol_90 = daily_returns.rolling(N).std() * np.sqrt(252)
        vol_90_safe = pd.Series(np.where(vol_90 == 0, np.nan, vol_90), index=df.index)

        # Combine everything
        hybrid_score = (adm_score * r2) / vol_90_safe

        # Stock Regime Filters
        stock_sma200 = close.rolling(200, min_periods=200).mean()
        stock_sma50 = close.rolling(50, min_periods=50).mean()
        stock_regime = (close > stock_sma200) & (close > stock_sma50)

        # Benchmark Regime Alignment
        regime = bench_regime.reindex(df.index).fillna(False)

        # Entry score: hybrid_score if positive and regimes are bull
        df["hybrid_score"] = np.where(
            regime & stock_regime & (hybrid_score > 0), hybrid_score, 0.0
        )
        prepared[symbol] = df

    return prepared


def _hybrid_lookback() -> int:
    return 252


@strategy(
    "hybrid_momentum",
    entry="hybrid_score > 0.0",
    exit="hybrid_score <= 0",
    prepare_bars=_prepare_hybrid,
    required_lookback=_hybrid_lookback,
)
def _hybrid_momentum() -> None:
    """Expression-only strategy. Body unused."""
