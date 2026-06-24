import pandas as pd
import numpy as np
from screener.strategies.spec import strategy, PrepareCtx

def prepare_risk_premia_forex(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Computes skewness of daily returns over the last 252 days.
    Long if skewness < -0.6
    Short if skewness > 0.6
    """
    for sym, df in ctx.bars_by_tv.items():
        if len(df) < 252:
            df["signal"] = 0
            continue
            
        ret_1d = df["close"].pct_change(1)
        skew_252 = ret_1d.rolling(252, min_periods=252).skew()
        
        signal = np.where(skew_252 < -0.6, 1, np.where(skew_252 > 0.6, -1, 0))
        df["signal"] = pd.Series(signal, index=df.index).fillna(0).astype(int)

    return ctx.bars_by_tv

@strategy(
    "qc_risk_premia_in_forex_markets",
    entry="signal == 1",
    exit="signal == 0",
    prepare_bars=prepare_risk_premia_forex,
    required_lookback=lambda: 252
)
def _qc_risk_premia_in_forex_markets():
    pass
