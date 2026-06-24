import pandas as pd
import numpy as np
from screener.strategies.spec import strategy, PrepareCtx


def prepare_leveraged_etfs(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    If multiple symbols are provided, assumes the first is Risk-On (e.g. SSO)
    and the second is Risk-Off (e.g. SHY). If only one is provided, uses its own SMA.
    """
    symbols = list(ctx.bars_by_tv.keys())
    if not symbols:
        return ctx.bars_by_tv

    risk_on_sym = symbols[0]
    risk_on_bars = ctx.bars_by_tv[risk_on_sym]

    if len(risk_on_bars) < 200:
        risk_on_sma200 = pd.Series(np.nan, index=risk_on_bars.index)
    else:
        risk_on_sma200 = risk_on_bars["close"].rolling(200, min_periods=200).mean()

    risk_on_signal = risk_on_bars["close"] > risk_on_sma200

    for i, sym in enumerate(symbols):
        df = ctx.bars_by_tv[sym]
        if i == 0:
            # Risk-On asset gets a 1 signal when risk_on_signal is True
            df["signal"] = risk_on_signal.reindex(df.index).fillna(False).astype(int)
        else:
            # Risk-Off asset gets a 1 signal when risk_on_signal is False
            # meaning we are below the 200 SMA
            df["signal"] = (~risk_on_signal).reindex(df.index).fillna(False).astype(int)

    return ctx.bars_by_tv


@strategy(
    "qc_leveraged_etfs_with_systematic_risk_management",
    entry="signal == 1",
    exit="signal == 0",
    prepare_bars=prepare_leveraged_etfs,
    required_lookback=lambda: 200,
)
def _qc_leveraged_etfs_with_systematic_risk_management():
    pass
