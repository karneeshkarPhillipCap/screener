"""Regime-Filtered Mean Reversion Strategy."""

from __future__ import annotations

from screener.strategies.spec import strategy


@strategy(
    "mean_reversion_regime",
    entry="rsi(close, 2) < 10 and close > sma(close, 200)",
    exit="close > sma(close, 5)",
)
def _mean_reversion_regime() -> None:
    """Expression-only strategy. Body unused."""
