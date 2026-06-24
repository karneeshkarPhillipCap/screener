"""Value Effect Within Countries strategy proxy."""

from __future__ import annotations

import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy

def _prepare_value_countries(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Since Shiller PE (CAPE) is unavailable, we proxy '10-year cheapness'
    by using a 5-year (1260 days) price reversal factor.
    Countries that have suffered the worst 5-year returns are considered the cheapest.
    The rule 'CAPE < 15' is proxied by requiring the 5-year return to be negative.
    We go long the top 33% cheapest countries, rebalancing monthly.
    """
    scores = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            continue
            
        close = bars["close"].astype(float)
        
        # 5-year return
        ret_5y = close / close.shift(1260) - 1.0
        scores[sym] = ret_5y
        
    if not scores:
        return ctx.bars_by_tv
        
    df_returns = pd.DataFrame(scores)
    df_returns_monthly = df_returns.resample("ME").last()
    
    # We want to buy the *lowest* 5-year returns
    # Rank ascending: lower return -> lower rank (e.g. 0.05)
    ranks = df_returns_monthly.rank(axis=1, pct=True)
    
    # Cheapest 33%: ranks <= 0.33
    # Absolute cheapness: ret_5y < 0
    long_monthly = (ranks <= 0.33) & (df_returns_monthly < 0.0)
    
    long_daily = long_monthly.reindex(df_returns.index, method="ffill").fillna(False)
    
    prepared = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[sym] = bars
            continue
            
        df = bars.copy().sort_index()
        df["value_long"] = long_daily.get(sym, pd.Series(False, index=df.index)).shift(1).fillna(False).astype(int)
        prepared[sym] = df
        
    return prepared

def _value_countries_lookback() -> int:
    return 1260

@strategy(
    "qc_value-effect-within-countries",
    entry="value_long > 0",
    exit="value_long == 0",
    prepare_bars=_prepare_value_countries,
    required_lookback=_value_countries_lookback,
)
def _qc_value_countries() -> None:
    """Expression-only strategy."""
