import pandas as pd
import numpy as np
from screener.strategies.spec import strategy, PrepareCtx

def prepare_improved_momentum(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Computes t-statistic of daily log-returns over past 252 days.
    Signal = 1 if t > 1, -1 if t < -1, else 0.
    """
    for sym, df in ctx.bars_by_tv.items():
        if len(df) < 252:
            df["signal"] = 0
            continue
            
        close = df["close"].astype(float)
        log_ret = np.log(close / close.shift(1))
        
        mean_ret = log_ret.rolling(252).mean()
        std_ret = log_ret.rolling(252).std()
        
        # Avoid division by zero
        std_ret = std_ret.replace(0, np.nan)
        
        t_stat = mean_ret / (std_ret / np.sqrt(252))
        
        signal = np.where(t_stat > 1.0, 1, np.where(t_stat < -1.0, -1, 0))
        df["signal"] = pd.Series(signal, index=df.index).fillna(0).astype(int)

    return ctx.bars_by_tv

@strategy(
    "qc_improved_momentum_strategy_on_commodities_futures",
    entry="signal == 1",
    exit="signal == 0",
    prepare_bars=prepare_improved_momentum,
    required_lookback=lambda: 252
)
def _qc_improved_momentum_strategy_on_commodities_futures():
    pass
