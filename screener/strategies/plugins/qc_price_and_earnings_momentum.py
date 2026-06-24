import pandas as pd
import numpy as np
from screener.strategies.spec import strategy, PrepareCtx

def prepare_price_momentum(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Approximates Price and Earnings Momentum by only using Price Momentum (63 days).
    Long top 10%, Short bottom 10%.
    """
    for sym, df in ctx.bars_by_tv.items():
        if len(df) < 63:
            df["mom_63d"] = np.nan
        else:
            df["mom_63d"] = df["close"].pct_change(63)
            
    # Create panel to rank
    mom_panel = pd.DataFrame({
        sym: df.get("mom_63d", pd.Series(np.nan, index=df.index))
        for sym, df in ctx.bars_by_tv.items()
    })
    
    # Rank daily across symbols
    ranks = mom_panel.rank(axis=1, ascending=False, pct=True)
    
    for sym, df in ctx.bars_by_tv.items():
        pct_rank = ranks[sym]
        # Top 10% get 1, Bottom 10% get -1
        signal = np.where(pct_rank <= 0.10, 1, np.where(pct_rank >= 0.90, -1, 0))
        df["signal"] = pd.Series(signal, index=df.index).fillna(0).astype(int)

    return ctx.bars_by_tv

@strategy(
    "qc_price_and_earnings_momentum",
    entry="signal == 1",
    exit="signal == 0",
    prepare_bars=prepare_price_momentum,
    required_lookback=lambda: 63
)
def _qc_price_and_earnings_momentum():
    pass
