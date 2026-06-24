"""Accelerating Dual Momentum (ADM) Strategy."""

from __future__ import annotations

import pandas as pd
import numpy as np

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_adm(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    benchmark_bars = ctx.price_panel.get(ctx.cfg.benchmark, pd.DataFrame())
    if benchmark_bars is None or benchmark_bars.empty:
        return ctx.bars_by_tv

    prepared = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue

        df = bars.copy().sort_index()
        close = df["close"].astype(float)

        # Calculate returns for 3-month (63), 6-month (126), 12-month (252)
        roc_63 = close / close.shift(63) - 1.0
        roc_126 = close / close.shift(126) - 1.0
        roc_252 = close / close.shift(252) - 1.0

        adm_score = (roc_63 + roc_126 + roc_252) / 3.0

        # Stock-specific regime filter (price > 200 SMA)
        stock_sma200 = close.rolling(200, min_periods=200).mean()
        stock_regime = close > stock_sma200

        # Entry score: adm_score if positive and stock is in bull regime
        df["adm_score"] = np.where(stock_regime & (adm_score > 0), adm_score, 0.0)
        prepared[symbol] = df

    return prepared


def _adm_lookback() -> int:
    return 252


@strategy(
    "accelerating_momentum",
    entry="adm_score > 0.0",
    exit="adm_score <= 0",
    prepare_bars=_prepare_adm,
    required_lookback=_adm_lookback,
)
def _accelerating_momentum() -> None:
    """Expression-only strategy. Body unused."""
