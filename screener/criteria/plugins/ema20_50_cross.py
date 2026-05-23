"""EMA20 above EMA50 with price confirming — the bullish crossover regime.

A point-in-time scan can't see the exact cross bar (that needs two bars), so the
screener surfaces names currently in the post-cross bullish state: EMA20 > EMA50
with close holding above EMA20. Works for both US and India scans.
"""

from __future__ import annotations

from tradingview_screener import col

from screener.criteria import criterion


@criterion("ema20_50_cross")
def ema20_above_ema50() -> list:
    return [
        col("EMA20") > col("EMA50"),
        col("close") > col("EMA20"),
        col("EMA50") > 0,
    ]
