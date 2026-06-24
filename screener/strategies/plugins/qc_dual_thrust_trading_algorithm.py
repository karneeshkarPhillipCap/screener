import pandas as pd
import numpy as np
from screener.strategies.spec import strategy, PrepareCtx


def prepare_dual_thrust(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """Calculate Dual Thrust range and thresholds."""
    k1 = 0.5
    k2 = 0.5
    n = 4
    for sym, df in ctx.bars_by_tv.items():
        if len(df) > n:
            hh = df["high"].rolling(n).max().shift(1)
            lc = df["close"].rolling(n).min().shift(1)
            hc = df["close"].rolling(n).max().shift(1)
            ll = df["low"].rolling(n).min().shift(1)

            rng = np.maximum(hh - lc, hc - ll)
            df["cap"] = df["open"] + k1 * rng
            df["floor"] = df["open"] - k2 * rng
        else:
            df["cap"] = np.nan
            df["floor"] = np.nan

    return ctx.bars_by_tv


@strategy(
    "qc_dual_thrust_trading_algorithm",
    entry="close > cap",
    exit="close < floor",
    prepare_bars=prepare_dual_thrust,
    required_lookback=lambda: 5,
)
def _qc_dual_thrust_trading_algorithm():
    pass
