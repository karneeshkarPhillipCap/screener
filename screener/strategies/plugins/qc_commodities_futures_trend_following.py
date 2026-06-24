import pandas as pd
import numpy as np
from screener.strategies.spec import strategy, PrepareCtx


def prepare_commodities_trend(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Trend following using 105-day EMA (approx 5 months).
    Signal is 1 if close > EMA, -1 if close < EMA.
    """
    for sym, df in ctx.bars_by_tv.items():
        if len(df) < 105:
            df["signal"] = 0
            continue

        ema_105 = df["close"].ewm(span=105, min_periods=105).mean()

        # 1 if price > ema, -1 if price < ema
        signal = np.where(
            df["close"] > ema_105, 1, np.where(df["close"] < ema_105, -1, 0)
        )
        df["signal"] = pd.Series(signal, index=df.index).fillna(0).astype(int)

    return ctx.bars_by_tv


@strategy(
    "qc_commodities_futures_trend_following",
    entry="signal == 1",
    exit="signal == 0",
    prepare_bars=prepare_commodities_trend,
    required_lookback=lambda: 105,
)
def _qc_commodities_futures_trend_following():
    pass
