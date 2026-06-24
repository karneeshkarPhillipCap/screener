"""Price Earnings Anomaly proxy strategy."""

from __future__ import annotations

import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy

def _prepare_pe_anomaly(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Since actual P/E fundamental data is unavailable, we use a classic behavioral
    proxy for 'Value': Long-term Reversal. Stocks that have experienced severe
    long-term underperformance (e.g., 3-year or 5-year negative returns) statistically 
    trade at compressed P/E multiples.
    We rebalance annually and select the top 10% most beaten-down stocks.
    """
    scores = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            continue
            
        close = bars["close"].astype(float)
        # 3-year (756 days) return, inverted so beaten down stocks have high score
        reversal_score = -(close / close.shift(756) - 1.0)
        scores[sym] = reversal_score
        
    if not scores:
        return ctx.bars_by_tv
        
    df_scores = pd.DataFrame(scores)
    
    # Annually rebalance at the end of the year ("beginning of each year")
    df_scores_annual = df_scores.resample("YE").last()
    
    ranks = df_scores_annual.rank(axis=1, pct=True)
    
    # Top 10% of value proxy
    long_annual = ranks >= 0.90
    
    long_daily = long_annual.reindex(df_scores.index, method="ffill").fillna(False)
    
    prepared = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[sym] = bars
            continue
            
        df = bars.copy().sort_index()
        df["pe_long"] = long_daily.get(sym, pd.Series(False, index=df.index)).shift(1).fillna(False).astype(int)
        prepared[sym] = df
        
    return prepared

def _pe_anomaly_lookback() -> int:
    return 756

@strategy(
    "qc_price-earnings-anomaly",
    entry="pe_long > 0",
    exit="pe_long == 0",
    prepare_bars=_prepare_pe_anomaly,
    required_lookback=_pe_anomaly_lookback,
)
def _qc_pe_anomaly() -> None:
    """Expression-only strategy."""
