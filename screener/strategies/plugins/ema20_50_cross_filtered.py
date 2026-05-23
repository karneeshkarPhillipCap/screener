"""EMA20/50 crossover with a long-term trend filter to cut drawdown.

Same cross entry as ``ema20_50_cross``, but only when price is in a confirmed
long-term uptrend (close above the 200 EMA) — this keeps the system out of
broad-market downturns. Exit on the bearish cross OR when the long-term trend
breaks (close drops below the 200 EMA), so regime breakdowns close positions
early instead of riding them down.
"""

from __future__ import annotations

from screener.strategies.spec import strategy


@strategy(
    "ema20_50_cross_filtered",
    entry=("crossover(ema(close, 20), ema(close, 50)) and close > ema(close, 200)"),
    exit="crossunder(close, ema(close, 50))",
)
def _ema20_50_cross_filtered() -> None:
    """Expression-only strategy. Body unused."""
