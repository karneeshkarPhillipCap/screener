"""Bollinger Bands: rolling SMA ± mult × rolling population std."""

from __future__ import annotations

import numpy as np
import pandas as pd

from screener.indicators.registry import indicator


@indicator("bb")
def bollinger_bands(
    x: np.ndarray, n: int = 20, mult: float = 2.0
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    roll = pd.Series(x).rolling(n, min_periods=n)
    middle = roll.mean().to_numpy()
    stds = roll.std(ddof=0).to_numpy()
    offset = stds * mult
    lower = middle - offset
    upper = middle + offset
    return lower, middle, upper
