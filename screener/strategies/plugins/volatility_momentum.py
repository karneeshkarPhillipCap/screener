"""Volatility-Adjusted Momentum Strategy."""

from __future__ import annotations

import pandas as pd
import numpy as np

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_volatility_momentum(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    benchmark_bars = ctx.price_panel.get(ctx.cfg.benchmark, pd.DataFrame())
    if benchmark_bars is None or benchmark_bars.empty:
        return ctx.bars_by_tv

    benchmark_close = benchmark_bars["close"].astype(float)
    bench_sma200 = benchmark_close.rolling(200, min_periods=200).mean()
    bench_regime = benchmark_close > bench_sma200

    prepared = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue
            
        df = bars.copy().sort_index()
        close = df["close"].astype(float)
        
        roc_90 = (close / close.shift(90) - 1.0)
        daily_returns = close.pct_change()
        vol_90 = daily_returns.rolling(90).std() * np.sqrt(252)
        
        # Avoid division by zero
        vol_90 = np.where(vol_90 == 0, np.nan, vol_90)
        vol_adj_score = roc_90 / vol_90
        
        # Align benchmark regime
        regime = bench_regime.reindex(df.index).fillna(False)
        
        # Entry score: vol_adj_score if positive and benchmark is in bull regime
        df["vol_adj_score"] = np.where(regime & (vol_adj_score > 0), vol_adj_score, 0.0)
        prepared[symbol] = df

    return prepared


def _vol_mom_lookback() -> int:
    return 200


@strategy(
    "volatility_momentum",
    entry="vol_adj_score > 0.0",
    exit="vol_adj_score <= 0",
    prepare_bars=_prepare_volatility_momentum,
    required_lookback=_vol_mom_lookback,
)
def _volatility_momentum() -> None:
    """Expression-only strategy. Body unused."""
