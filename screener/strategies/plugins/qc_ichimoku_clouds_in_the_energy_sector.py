import pandas as pd
import numpy as np
from screener.strategies.spec import PrepareCtx, strategy


def _prepare_qc_ichimoku_clouds_in_the_energy_sector(
    ctx: PrepareCtx,
) -> dict[str, pd.DataFrame]:
    prepared = {}

    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue

        df = bars.copy().sort_index()

        high = df["high"]
        low = df["low"]
        close = df["close"]

        tenkan_sen = (high.rolling(window=9).max() + low.rolling(window=9).min()) / 2
        kijun_sen = (high.rolling(window=26).max() + low.rolling(window=26).min()) / 2

        senkou_a = ((tenkan_sen + kijun_sen) / 2).shift(26)
        senkou_b = (
            (high.rolling(window=52).max() + low.rolling(window=52).min()) / 2
        ).shift(26)

        cloud_top = np.maximum(senkou_a, senkou_b)
        cloud_bottom = np.minimum(senkou_a, senkou_b)

        # Location: 1 if above cloud, -1 if below cloud, 0 otherwise
        location = pd.Series(0, index=df.index)
        location[close > cloud_top] = 1
        location[close < cloud_bottom] = -1

        prev_location = location.shift(1)

        # Long when Chikou (close) crosses top of the cloud from below (or inside)
        df["entry_signal"] = (location == 1) & (prev_location != 1)

        # Short (exit long) when Chikou crosses bottom of the cloud from above (or inside)
        df["exit_signal"] = (location == -1) & (prev_location != -1)

        prepared[symbol] = df

    return prepared


def _lookback_qc_ichimoku_clouds_in_the_energy_sector() -> int:
    return 100  # 52 window + 26 shift = 78 days minimum. 100 is safe.


@strategy(
    "qc_ichimoku_clouds_in_the_energy_sector",
    entry="entry_signal",
    exit="exit_signal",
    prepare_bars=_prepare_qc_ichimoku_clouds_in_the_energy_sector,
    required_lookback=_lookback_qc_ichimoku_clouds_in_the_energy_sector,
)
def _qc_ichimoku_clouds_in_the_energy_sector() -> None:
    pass
