"""Time Series Momentum Effect strategy."""

from __future__ import annotations

import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_tsmom(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Standard Time Series Momentum (TSMOM) effect.
    The asset is bought if its 12-month return is positive, and shorted if negative.
    (Volatility scaling is common but skipped here for simplicity in pure expression signals).
    """
    prepared = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[sym] = bars
            continue

        df = bars.copy().sort_index()
        close = df["close"].astype(float)

        # 12-month (252-day) return
        ret_12m = close / close.shift(252) - 1.0

        # Rebalance monthly: resample signal to end of month, then forward fill
        ret_12m_monthly = ret_12m.resample("ME").last()
        long_monthly = ret_12m_monthly > 0
        short_monthly = ret_12m_monthly < 0

        long_daily = long_monthly.reindex(df.index, method="ffill").fillna(False)
        short_daily = short_monthly.reindex(df.index, method="ffill").fillna(False)

        df["tsmom_long"] = long_daily.shift(1).fillna(False).astype(int)
        df["tsmom_short"] = short_daily.shift(1).fillna(False).astype(int)
        df["tsmom_signal"] = df["tsmom_long"] - df["tsmom_short"]

        prepared[sym] = df

    return prepared


def _tsmom_lookback() -> int:
    return 252


@strategy(
    "qc_time-series-momentum-effect",
    entry="tsmom_signal > 0",
    exit="tsmom_signal == 0",
    prepare_bars=_prepare_tsmom,
    required_lookback=_tsmom_lookback,
)
def _qc_tsmom() -> None:
    """Expression-only strategy."""
