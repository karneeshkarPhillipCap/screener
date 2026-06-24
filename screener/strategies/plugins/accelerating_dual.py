"""Accelerating Dual Momentum Strategy (Winner of random combination search)."""

from __future__ import annotations

import pandas as pd
from screener.strategies.spec import PrepareCtx, strategy, registry


def _prepare_accel_dual(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    # Run Accelerating Momentum preparation
    spec_adm = registry.get("accelerating_momentum")
    if spec_adm and spec_adm.prepare_bars:
        ctx.bars_by_tv = spec_adm.prepare_bars(ctx)

    # Run Dual Momentum preparation
    spec_dual = registry.get("dual_momentum")
    if spec_dual and spec_dual.prepare_bars:
        ctx.bars_by_tv = spec_dual.prepare_bars(ctx)

    return ctx.bars_by_tv


def _lookback_accel_dual() -> int:
    return 252


@strategy(
    "accelerating_dual",
    entry="(adm_score > 0.0) and (dual_momentum_score > 0.1)",
    exit="(adm_score <= 0) or (dual_momentum_score <= 0)",
    prepare_bars=_prepare_accel_dual,
    required_lookback=_lookback_accel_dual,
)
def _accel_dual_strat() -> None:
    pass
