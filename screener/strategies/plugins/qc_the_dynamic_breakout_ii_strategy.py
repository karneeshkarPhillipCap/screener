"""The Dynamic Breakout II Strategy."""

from __future__ import annotations

import numpy as np
import pandas as pd

from screener.strategies.spec import strategy
from screener.strategies.trades import Trade, _walk


@strategy("qc_the_dynamic_breakout_ii_strategy")
def strat_qc_the_dynamic_breakout_ii_strategy(df: pd.DataFrame) -> list[Trade]:
    """
    Implements The Dynamic Breakout II Strategy.

    This strategy dynamically adjusts its lookback period (N) based on changes in market volatility.
    It buys when the previous close breaks above the N-day upper Bollinger Band and today's high breaks
    the N-day highest high. It exits when the close drops below the N-day SMA.
    """
    close = df["close"].to_numpy(dtype=float)
    high = df["high"].to_numpy(dtype=float)
    dates = df["date"].values

    n_days = len(close)
    entries = np.zeros(n_days, dtype=bool)
    exits = np.zeros(n_days, dtype=bool)

    if n_days < 60:
        return []

    N_current = 20

    for i in range(20, n_days):
        # Lookback period N used for today's trading decisions
        N = N_current

        if i >= N:
            # 1. Check Entry
            # "close price of previous day must be above the upper Bollinger Band"
            bb_window = close[i - N : i]
            bb_mean = np.mean(bb_window)
            bb_std = np.std(bb_window) if len(bb_window) > 1 else 0.0
            upper_bb = bb_mean + 2 * bb_std

            cond1 = close[i - 1] > upper_bb

            # "ask price must be above the highest high of the most recent N days"
            highest_high = np.max(high[i - N : i])
            cond2 = high[i] > highest_high

            if cond1 and cond2:
                entries[i] = True

            # 2. Check Exit
            # "liquidate a long position if the current price is lower than the moving average"
            current_window = close[i - N + 1 : i + 1]
            if len(current_window) > 0:
                ma_current = np.mean(current_window)
                if close[i] < ma_current:
                    exits[i] = True

        # 3. Update N for the next day
        if i >= N:
            todayvol = np.std(close[i - N + 1 : i + 1])
            yesterdayvol = np.std(close[i - N : i])

            if todayvol > 0:
                deltavol = (todayvol - yesterdayvol) / todayvol
                N_next = round(N * (1 + deltavol))
                # Restrict within acceptable range [20, 60]
                N_current = int(max(20, min(N_next, 60)))

    return _walk(entries, exits, close, dates)
