import pandas as pd
import numpy as np
from screener.strategies.spec import strategy, PrepareCtx

def prepare_dynamic_pairs(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Proxy implementation for Intraday Dynamic Pairs Trading.
    Calculates a cross-sectional market proxy (mean price of universe).
    Calculates z-score of the residual (price - market_proxy).
    Buys if z-score < -2.0, Shorts if z-score > 2.0.
    """
    # Create panel of close prices
    valid_symbols = [sym for sym, df in ctx.bars_by_tv.items() if len(df) > 90]
    
    if len(valid_symbols) == 0:
        for sym, df in ctx.bars_by_tv.items():
            df["signal"] = 0
        return ctx.bars_by_tv
        
    panel = pd.DataFrame({sym: ctx.bars_by_tv[sym]["close"] for sym in valid_symbols})
    
    # Calculate market proxy (mean of normalized prices to avoid high price stocks dominating)
    # Use rolling 90-day window for normalization to adapt to changes
    norm_panel = panel / panel.rolling(90, min_periods=1).mean()
    market_proxy = norm_panel.mean(axis=1)
    
    for sym in valid_symbols:
        df = ctx.bars_by_tv[sym]
        
        # Residual of normalized price vs market proxy
        residual = norm_panel[sym] - market_proxy
        
        # Calculate rolling 60-day z-score of residual
        roll_mean = residual.rolling(60).mean()
        roll_std = residual.rolling(60).std()
        
        z_score = (residual - roll_mean) / roll_std
        
        # Generate signals
        signal = pd.Series(0, index=df.index)
        
        # Long if extremely undervalued relative to sector
        signal[z_score < -2.0] = 1
        
        # Short if extremely overvalued relative to sector
        signal[z_score > 2.0] = -1
        
        # Ffill the state until it reverts back to mean
        position = signal.replace(0, np.nan)
        position[np.abs(z_score) < 0.5] = 0
        position = position.ffill().fillna(0)
        
        df["signal"] = position
        
    for sym, df in ctx.bars_by_tv.items():
        if "signal" not in df.columns:
            df["signal"] = 0
            
    return ctx.bars_by_tv

@strategy(
    "qc_intraday-dynamic-pairs-trading-using-correlation-and-cointegration-approach",
    entry="signal != 0",
    exit="signal == 0",
    prepare_bars=prepare_dynamic_pairs,
    required_lookback=lambda: 150 # 90 days for normalization + 60 days for z-score
)
def _qc_dynamic_pairs_trading():
    pass
