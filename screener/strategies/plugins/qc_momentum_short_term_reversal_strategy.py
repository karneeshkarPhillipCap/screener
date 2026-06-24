import pandas as pd
import numpy as np

from screener.strategies.spec import PrepareCtx, strategy

def _prepare_mom_reversal(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    closes = {}
    for sym, bars in ctx.bars_by_tv.items():
        if not bars.empty:
            closes[sym] = bars["close"].astype(float)
            
    if not closes:
        return ctx.bars_by_tv
        
    df_close = pd.DataFrame(closes)
    
    # 1. Price > $10 filter
    price_filter = df_close > 10.0
    
    # 2. 12-month return and GARR_12
    ret_12m = df_close / df_close.shift(252) - 1.0
    garr_12 = (1 + ret_12m) ** (1/12) - 1.0
    
    # 3. 1-month return and GARR_1
    ret_1m = df_close / df_close.shift(21) - 1.0
    garr_1 = (1 + ret_1m) ** (1/12) - 1.0
    
    # 4. GARR Ratio
    garr_ratio = garr_1 / garr_12
    
    # 5. Winner Group (Top 30% of 12-month return)
    # Mask non-qualifying prices with NaN
    valid_ret_12m = ret_12m.where(price_filter, np.nan)
    ranks_12m = valid_ret_12m.rank(axis=1, pct=True)
    is_winner = ranks_12m >= 0.70
    
    # 6. Among winners, find 15 with LOWEST GARR ratio
    # Mask GARR ratio for non-winners
    winner_garr_ratio = garr_ratio.where(is_winner, np.nan)
    garr_ranks = winner_garr_ratio.rank(axis=1, ascending=True)
    
    # Select top 15
    is_long = garr_ranks <= 15
    
    prepared = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars.empty:
            prepared[sym] = bars
            continue
            
        df = bars.copy()
        df["mom_rev_entry"] = is_long[sym].reindex(df.index).fillna(False)
        df["mom_rev_exit"] = ~df["mom_rev_entry"]
        prepared[sym] = df
        
    return prepared

def _mom_reversal_lookback() -> int:
    return 252

@strategy(
    "qc_momentum_short_term_reversal_strategy",
    entry="mom_rev_entry",
    exit="mom_rev_exit",
    prepare_bars=_prepare_mom_reversal,
    required_lookback=_mom_reversal_lookback,
)
def _mom_reversal() -> None:
    """
    Momentum Short Term Reversal Strategy.
    Longs the 15 stocks from the 12-month winner group (top 30%)
    that have the lowest GARR ratio (short-term pullback).
    """
    pass
