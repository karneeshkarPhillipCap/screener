import pandas as pd
import numpy as np

from screener.strategies.spec import PrepareCtx, strategy

def _prepare_paired_switching(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    closes = {}
    for sym, bars in ctx.bars_by_tv.items():
        if not bars.empty:
            closes[sym] = bars["close"].astype(float)
            
    if not closes:
        return ctx.bars_by_tv
        
    df_close = pd.DataFrame(closes)
    
    # 90 calendar days is approx 63 trading days
    ret_90d = df_close / df_close.shift(63) - 1.0
    
    # Rank cross-sectionally
    ranks = ret_90d.rank(axis=1, ascending=False)
    
    # Select the single best performer
    is_top_1 = ranks == 1.0
    
    prepared = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars.empty:
            prepared[sym] = bars
            continue
            
        df = bars.copy()
        df["paired_entry"] = is_top_1[sym].reindex(df.index).fillna(False)
        df["paired_exit"] = ~df["paired_entry"]
        prepared[sym] = df
        
    return prepared

def _paired_switching_lookback() -> int:
    return 63

@strategy(
    "qc_paired_switching",
    entry="paired_entry",
    exit="paired_exit",
    prepare_bars=_prepare_paired_switching,
    required_lookback=_paired_switching_lookback,
)
def _paired_switching() -> None:
    """
    Paired Switching.
    Allocates 100% to the asset with the highest 90-day return.
    Typically used with a universe of exactly 2 negatively correlated assets.
    """
    pass
