"""Beta Factor In Country Equity Indexes strategy."""

from __future__ import annotations

import numpy as np
import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy

def _prepare_beta_factor(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Betting Against Beta strategy.
    Calculates the 1-year rolling beta of each asset against a market index (SPY, if available,
    otherwise an equal-weighted proxy).
    Ranks the assets cross-sectionally by their beta.
    Goes long the bottom 25% (low-beta) and shorts the top 25% (high-beta).
    """
    closes = {sym: bars["close"].astype(float) for sym, bars in ctx.bars_by_tv.items() if bars is not None and not bars.empty}
    if not closes:
        return ctx.bars_by_tv
        
    df_closes = pd.DataFrame(closes).ffill()
    daily_rets = df_closes.pct_change()
    
    # Try to find SPY in price panel, otherwise use equal-weighted mean
    market_ret = None
    if ctx.cfg.benchmark and ctx.cfg.benchmark in ctx.price_panel and not ctx.price_panel[ctx.cfg.benchmark].empty:
        bench_close = ctx.price_panel[ctx.cfg.benchmark]["close"].astype(float)
        market_ret = bench_close.pct_change().reindex(daily_rets.index)
    elif "SPY" in ctx.price_panel and not ctx.price_panel["SPY"].empty:
        spy_close = ctx.price_panel["SPY"]["close"].astype(float)
        market_ret = spy_close.pct_change().reindex(daily_rets.index)
    else:
        market_ret = daily_rets.mean(axis=1)
        
    market_var = market_ret.rolling(252, min_periods=63).var()
    
    betas = {}
    for sym in daily_rets.columns:
        cov = daily_rets[sym].rolling(252, min_periods=63).cov(market_ret)
        betas[sym] = cov / market_var
        
    df_betas = pd.DataFrame(betas)
    
    # Monthly rebalance
    df_betas_monthly = df_betas.resample("ME").last()
    ranks = df_betas_monthly.rank(axis=1, pct=True)
    
    # Long low beta (bottom 25%), short high beta (top 25%)
    long_monthly = ranks <= 0.25
    short_monthly = ranks >= 0.75
    
    long_daily = long_monthly.reindex(df_closes.index, method="ffill").fillna(False)
    short_daily = short_monthly.reindex(df_closes.index, method="ffill").fillna(False)
    
    prepared = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[sym] = bars
            continue
            
        df = bars.copy().sort_index()
        df["beta_long"] = long_daily.get(sym, pd.Series(False, index=df.index)).shift(1).fillna(False).astype(int)
        df["beta_short"] = short_daily.get(sym, pd.Series(False, index=df.index)).shift(1).fillna(False).astype(int)
        df["beta_signal"] = df["beta_long"] - df["beta_short"]
        
        prepared[sym] = df
        
    return prepared

def _beta_lookback() -> int:
    return 252

@strategy(
    "qc_beta-factor-in-country-equity-indexes",
    entry="beta_signal > 0",
    exit="beta_signal == 0",
    prepare_bars=_prepare_beta_factor,
    required_lookback=_beta_lookback,
)
def _qc_beta_factor() -> None:
    """Expression-only strategy."""
