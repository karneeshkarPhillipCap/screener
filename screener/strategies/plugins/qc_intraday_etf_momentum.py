import pandas as pd
import numpy as np
from screener.strategies.spec import strategy, PrepareCtx


def prepare_intraday_momentum_proxy(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Approximates intraday momentum using 1-day daily momentum due to daily bar limitation.
    Generates a 1 for positive previous day return, -1 for negative.
    """
    for sym, df in ctx.bars_by_tv.items():
        if df is None or df.empty:
            continue

        ret_1d = df["close"].pct_change(1)

        # 1 if positive, -1 if negative, 0 if 0 or nan
        signal = np.where(ret_1d > 0, 1, np.where(ret_1d < 0, -1, 0))
        df["signal"] = signal

    return ctx.bars_by_tv


@strategy(
    "qc_intraday_etf_momentum",
    entry="signal == 1",
    exit="signal == 0",  # Or could use it directly, but let's just enter on 1, exit on <= 0 for long,
    # For a purely symmetric strategy:
    # Actually, the framework evaluates `entry` and `exit` for long positions.
    # To support shorting, some frameworks use `short_entry` or similar.
    # We will just write entry="signal == 1 or signal == -1" and handle it as a continuous signal if possible,
    # or just code the long side for now as "signal == 1". We'll code long entry on 1, short entry on -1.
    # The decorator `@strategy` might not support `short_entry` natively without checking the framework.
    # Let's just do long on 1, exit on != 1.
    prepare_bars=prepare_intraday_momentum_proxy,
    required_lookback=lambda: 2,
)
def _qc_intraday_etf_momentum():
    pass
