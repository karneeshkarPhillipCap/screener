"""EMA20/EMA50 crossover: buy when EMA20 crosses above EMA50, sell on cross-under.

The bullish crossover is treated as the breakout entry; the position is held
until the trend condition stops holding (EMA20 crosses back below EMA50).
"""

from __future__ import annotations

from screener.strategies.spec import strategy


@strategy(
    "ema20_50_cross",
    entry="crossover(ema(close, 20), ema(close, 50))",
    exit="crossunder(ema(close, 20), ema(close, 50))",
)
def _ema20_50_cross() -> None:
    """Expression-only strategy. Body unused."""
