import pandas as pd
import numpy as np

from screener.strategies.spec import PrepareCtx, strategy

def _prepare_commodity_mom_term(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    closes = {}
    for sym, bars in ctx.bars_by_tv.items():
        if not bars.empty:
            closes[sym] = bars["close"].astype(float)
            
    if not closes:
        return ctx.bars_by_tv
        
    df_close = pd.DataFrame(closes)
    
    # Proxy for term structure (backwardation vs contango)
    sma_252 = df_close.rolling(252, min_periods=126).mean()
    term_proxy = df_close / sma_252 - 1.0
    
    # 1-month momentum
    mom_1m = df_close / df_close.shift(21) - 1.0
    
    # 1. Split term_proxy into tertiles
    term_ranks = term_proxy.rank(axis=1, pct=True)
    is_high_term = term_ranks >= 0.6667
    
    # 2. In High group, find the winners (top 50% momentum)
    # Mask non-high term structure commodities
    high_mom_1m = mom_1m.where(is_high_term, np.nan)
    mom_ranks = high_mom_1m.rank(axis=1, pct=True)
    
    # Since only ~33% of assets are in the High group, the valid ranks range from 0 to 1 among them.
    # The top 50% of this subgroup means mom_ranks >= 0.50
    is_high_winner = is_high_term & (mom_ranks >= 0.50)
    
    prepared = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars.empty:
            prepared[sym] = bars
            continue
            
        df = bars.copy()
        df["comm_entry"] = is_high_winner[sym].reindex(df.index).fillna(False)
        df["comm_exit"] = ~df["comm_entry"]
        prepared[sym] = df
        
    return prepared

def _commodity_mom_term_lookback() -> int:
    return 252

@strategy(
    "qc_momentum_effect_combined_with_term_structure_in_commodities",
    entry="comm_entry",
    exit="comm_exit",
    prepare_bars=_prepare_commodity_mom_term,
    required_lookback=_commodity_mom_term_lookback,
)
def _commodity_mom_term() -> None:
    """
    Momentum Effect Combined with Term Structure in Commodities.
    Approximated roll yield (term structure) using long-term moving average deviation.
    Approximated the short leg out (long-only).
    """
    pass
