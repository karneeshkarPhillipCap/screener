import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy

def _prepare_accrual_anomaly(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    volatility_dict = {}
    
    # Calculate 1-year volatility for each symbol as a proxy for earnings quality
    for sym, bars in ctx.bars_by_tv.items():
        if bars.empty:
            continue
            
        df = bars.copy()
        close = df["close"].astype(float)
        
        # 1-year (252 days) daily return standard deviation
        pct_change = close.pct_change()
        vol = pct_change.rolling(252, min_periods=126).std()
        
        volatility_dict[sym] = vol

    if not volatility_dict:
        return ctx.bars_by_tv
        
    vol_df = pd.DataFrame(volatility_dict)
    
    # We want the bottom decile (lowest volatility = proxy for lowest accruals / highest quality)
    ranks = vol_df.rank(axis=1, pct=True)
    is_bottom_decile = ranks <= 0.10
    
    prepared = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars.empty:
            prepared[sym] = bars
            continue
            
        df = bars.copy()
        df["accrual_entry"] = is_bottom_decile[sym].reindex(df.index).fillna(False)
        df["accrual_exit"] = ~df["accrual_entry"]
        prepared[sym] = df
        
    return prepared

def _accrual_anomaly_lookback() -> int:
    return 252

@strategy(
    "qc_accrual_anomaly",
    entry="accrual_entry",
    exit="accrual_exit",
    prepare_bars=_prepare_accrual_anomaly,
    required_lookback=_accrual_anomaly_lookback,
)
def _accrual_anomaly() -> None:
    """
    Accrual Anomaly.
    Approximated using 1-year price volatility as a proxy for earnings quality 
    due to lack of fundamental data. Longs the bottom decile of volatility.
    """
    pass
