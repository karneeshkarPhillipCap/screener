"""Volatility Contraction Breakout (TTM Squeeze)."""

from __future__ import annotations

import pandas as pd

from screener.indicators.plugins.sma import sma as _sma
from screener.indicators.plugins.ema import ema as _ema
from screener.indicators.plugins.atr import atr as _atr
from screener.indicators.plugins.bollinger_bands import bollinger_bands as _bb
from screener.strategies.spec import PrepareCtx, strategy


def _prepare_vcp(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    prepared = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue
        df = bars.copy().sort_index()
        close = df["close"].to_numpy(dtype=float)
        high = df["high"].to_numpy(dtype=float)
        low = df["low"].to_numpy(dtype=float)
        vol = df["volume"].to_numpy(dtype=float)

        bb_lower, bb_middle, bb_upper = _bb(close, 20, 2.0)
        kc_middle = _ema(close, 20)
        atr_20 = _atr(high, low, close, 20)
        kc_upper = kc_middle + 1.5 * atr_20
        kc_lower = kc_middle - 1.5 * atr_20

        squeeze = (bb_upper < kc_upper) & (bb_lower > kc_lower)
        squeeze_series = pd.Series(squeeze).rolling(5).max() > 0

        vol_sma = _sma(vol, 20)
        vol_breakout = vol > vol_sma

        price_breakout = close > bb_upper

        df["vcp_entry"] = (
            squeeze_series.to_numpy() & price_breakout & vol_breakout
        ).astype(float)
        prepared[symbol] = df
    return prepared


def _vcp_lookback() -> int:
    return 25


@strategy(
    "vcp_breakout",
    entry="vcp_entry > 0",
    exit="close < sma(close, 20)",
    prepare_bars=_prepare_vcp,
    required_lookback=_vcp_lookback,
)
def _vcp_breakout() -> None:
    """Expression-only strategy."""
