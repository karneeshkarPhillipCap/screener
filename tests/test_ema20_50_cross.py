"""EMA20/EMA50 crossover strategy + screener criterion."""

from __future__ import annotations

import numpy as np
import pandas as pd

from screener.backtester.pine import evaluate, parse
from screener.backtester.strategies import resolve_strategy
from screener.criteria import CRITERIA


def test_strategy_resolves_to_crossover_expressions():
    strat = resolve_strategy("ema20_50_cross")
    assert strat.entry == "crossover(ema(close, 20), ema(close, 50))"
    assert strat.exit == "crossunder(ema(close, 20), ema(close, 50))"


def test_criterion_registered_with_bullish_filters():
    assert "ema20_50_cross" in CRITERIA
    filters = CRITERIA["ema20_50_cross"]()
    assert len(filters) == 3  # EMA20>EMA50, close>EMA20, EMA50>0


def test_filtered_variant_adds_trend_filter_and_faster_exit():
    strat = resolve_strategy("ema20_50_cross_filtered")
    # Trend regime filter on entry; exit closes the moment price loses EMA50.
    assert "close > ema(close, 200)" in strat.entry
    assert "crossover(ema(close, 20), ema(close, 50))" in strat.entry
    assert strat.exit == "crossunder(close, ema(close, 50))"


def _bars_down_up_down() -> pd.DataFrame:
    """Close that declines, rallies, then declines — forces a cross up then down."""
    seg = lambda a, b, n: np.linspace(a, b, n)  # noqa: E731
    close = np.concatenate([seg(120, 90, 80), seg(90, 160, 120), seg(160, 100, 120)])
    return pd.DataFrame(
        {
            "open": close,
            "high": close + 1.0,
            "low": close - 1.0,
            "close": close,
            "volume": np.full(len(close), 10_000.0),
        }
    )


def test_entry_and_exit_fire_on_synthetic_cross():
    bars = _bars_down_up_down()
    strat = resolve_strategy("ema20_50_cross")

    entries = evaluate(parse(strat.entry), bars)
    exits = evaluate(parse(strat.exit), bars)

    assert bool(entries.any()), "EMA20 should cross above EMA50 during the rally"
    assert bool(exits.any()), "EMA20 should cross below EMA50 during the decline"
    # The bullish cross must precede the bearish cross.
    assert entries[entries].index[0] < exits[exits].index[0]
