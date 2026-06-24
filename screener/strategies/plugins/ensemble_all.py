"""Ensemble All Strategy."""

from __future__ import annotations

import pandas as pd
import numpy as np

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_ensemble_all(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    from screener.strategies.spec import registry
    
    # Run all prepare_bars from the registry to accumulate signals
    for name, spec in registry.items():
        if name == "ensemble_all":
            continue
        if spec.prepare_bars is not None:
            try:
                # We update the context in-place as each plugin adds its columns
                ctx.bars_by_tv = spec.prepare_bars(ctx)
            except Exception:
                pass
                
    prepared = {}
    for symbol, df in ctx.bars_by_tv.items():
        if df is None or df.empty:
            prepared[symbol] = df
            continue
            
        votes = pd.Series(0.0, index=df.index)
        
        # We count votes for all major quantitative strategy scores
        if "clenow_score" in df.columns:
            votes += (df["clenow_score"] > 0.05).astype(float)
        if "adm_score" in df.columns:
            votes += (df["adm_score"] > 0).astype(float)
        if "vol_adj_score" in df.columns:
            votes += (df["vol_adj_score"] > 0).astype(float)
        if "vcp_entry" in df.columns:
            votes += (df["vcp_entry"] > 0).astype(float)
        if "pm_score" in df.columns:
            votes += (df["pm_score"] > 0).astype(float)
        if "hybrid_score" in df.columns:
            votes += (df["hybrid_score"] > 0).astype(float)
        if "omni_score" in df.columns:
            votes += (df["omni_score"] > 0).astype(float)
        if "ultimate_score" in df.columns:
            votes += (df["ultimate_score"] > 0).astype(float)
            
        # Tie-breaker: use pure clenow score as a micro-boost so the screener can still rank within equal votes
        tie_breaker = df.get("clenow_score", pd.Series(0.0, index=df.index)) * 0.001
        
        # Only enter if we get at least 3 distinct strategy confirmations
        df["ensemble_score"] = np.where(votes >= 3, votes + tie_breaker, 0.0)
        prepared[symbol] = df

    return prepared


def _ensemble_lookback() -> int:
    return 252


@strategy(
    "ensemble_all",
    entry="ensemble_score >= 3.0",
    exit="ensemble_score == 0",
    prepare_bars=_prepare_ensemble_all,
    required_lookback=_ensemble_lookback,
)
def _ensemble_all_strat() -> None:
    """Expression-only strategy. Body unused."""
