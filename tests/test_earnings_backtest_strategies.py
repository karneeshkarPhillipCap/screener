from __future__ import annotations

import pandas as pd

from screener.earnings_backtest.strategies import price_momentum, volume_surge


def _bars() -> pd.DataFrame:
    dates = pd.bdate_range("2026-01-01", periods=25)
    close = [100.0] * len(dates)
    volume = [100.0] * len(dates)
    close[-2] = 90.0
    close[-1] = 120.0
    volume[-1] = 1_000.0
    return pd.DataFrame(
        {
            "open": close,
            "high": close,
            "low": close,
            "close": close,
            "volume": volume,
        },
        index=dates,
    )


def test_price_momentum_uses_entry_date_as_signal_date() -> None:
    bars = _bars()
    result = price_momentum(
        "TEST",
        bars.index[-1],
        bars,
        threshold=0.5,
        as_of_date=bars.index[-2],
    )

    assert result.passed is False
    assert result.score == 0.0
    assert result.details["signal_date"] == bars.index[-2].date().isoformat()


def test_volume_surge_uses_entry_date_as_signal_date() -> None:
    bars = _bars()
    result = volume_surge(
        "TEST",
        bars.index[-1],
        bars,
        threshold=0.5,
        as_of_date=bars.index[-2],
    )

    assert result.passed is False
    assert result.details["signal_date"] == bars.index[-2].date().isoformat()
