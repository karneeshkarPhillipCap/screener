"""Stocks between 80–100% of the 52-week high but strictly below that high (under resistance)."""

from __future__ import annotations

from tradingview_screener import col

from screener.criteria import criterion


@criterion("near_52_high")
def near_52_week_high() -> list:
    return [
        col("close").between_pct("price_52_week_high", 0.8, 1),
        col("close") < col("price_52_week_high"),
    ]
