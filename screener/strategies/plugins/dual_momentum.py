"""Cross-Sectional / Dual Momentum Strategy."""

from __future__ import annotations

import pandas as pd
import numpy as np

from screener.strategies.spec import PrepareCtx, strategy

def _prepare_dual_momentum(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
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
        
        # 90-day ROC
        roc_90 = (close / close.shift(90) - 1.0)
        
        # Align benchmark regime
        regime = bench_regime.reindex(df.index).fillna(False)
        
        # Entry score: ROC 90 if positive and benchmark is in bull regime
        df["dual_momentum_score"] = np.where(regime & (roc_90 > 0), roc_90, 0.0)
        prepared[symbol] = df

    return prepared

def _dual_momentum_lookback() -> int:
    return 200

@strategy(
    "dual_momentum",
    entry="dual_momentum_score > 0.1",
    exit="dual_momentum_score <= 0",
    prepare_bars=_prepare_dual_momentum,
    required_lookback=_dual_momentum_lookback,
)
def _dual_momentum() -> None:
    """Expression-only strategy. Body unused."""
