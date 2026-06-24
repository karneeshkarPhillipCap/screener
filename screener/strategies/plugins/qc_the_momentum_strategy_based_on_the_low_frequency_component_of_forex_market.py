"""The Momentum Strategy Based On The Low Frequency Component Of Forex Market."""

from __future__ import annotations

import numpy as np
import pandas as pd
from scipy import sparse
from scipy.sparse.linalg import factorized

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_momentum_hpfilter(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Computes rolling Hodrick-Prescott filter on price and generates
    buy/sell signals based on MA(1,2) of the non-linear trend component.
    """
    lamb = 100
    window = 1800

    nobs = window
    identity_mat = sparse.eye(nobs, nobs)
    offsets = np.array([0, 1, 2])
    data = np.repeat([[1.0], [-2.0], [1.0]], nobs, axis=1)
    K = sparse.dia_matrix((data, offsets), shape=(nobs - 2, nobs))
    mat = identity_mat + lamb * K.T.dot(K)
    solve = factorized(mat.tocsc())

    for sym, df in ctx.bars_by_tv.items():
        if df is None or df.empty:
            continue

        n = len(df)
        signal = np.zeros(n)

        if n < window:
            df["position_target"] = 0
            continue

        prices = df["close"].values

        for i in range(window, n + 1):
            window_data = prices[i - window : i]
            trend = solve(window_data)

            # MA(1,2) on the trend curve estimated TODAY
            # m=1, n=2
            ma_today = trend[-1] - (trend[-2] + trend[-1]) / 2.0
            ma_yest = trend[-2] - (trend[-3] + trend[-2]) / 2.0

            if ma_today > 0 and ma_yest < 0:
                signal[i - 1] = 1
            elif ma_today < 0 and ma_yest > 0:
                signal[i - 1] = -1

        # Forward fill the signal to hold the position until a new signal flips it
        # We start with 0, fill with NaN where 0, then forward fill, then fill remaining with 0
        sig_series = (
            pd.Series(signal, index=df.index).replace(0, np.nan).ffill().fillna(0)
        )
        df["position_target"] = sig_series

    return ctx.bars_by_tv


def _lookback() -> int:
    return 1800


@strategy(
    "qc_the_momentum_strategy_based_on_the_low_frequency_component_of_forex_market",
    entry="position_target == 1",
    exit="position_target == -1",
    direction="both",
    prepare_bars=_prepare_momentum_hpfilter,
    required_lookback=_lookback,
)
def _qc_the_momentum_strategy_based_on_the_low_frequency_component_of_forex_market() -> (
    None
):
    """The Momentum Strategy Based On The Low Frequency Component Of Forex Market."""
