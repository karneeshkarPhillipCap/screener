import pandas as pd
import numpy as np
from screener.strategies.spec import strategy, PrepareCtx


def prepare_mf_momentum(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Standard 12-month momentum for mutual funds / ETFs.
    Ranks funds based on 252-day return and emits signal=1 for the top fund.
    """
    for sym, df in ctx.bars_by_tv.items():
        if len(df) >= 252:
            df["mom_12m"] = df["close"].pct_change(252)
        else:
            df["mom_12m"] = np.nan

    # Create a panel to rank across symbols
    mom_panel = pd.DataFrame({sym: df["mom_12m"] for sym, df in ctx.bars_by_tv.items()})

    # Rank daily across symbols (highest return = rank 1)
    ranks = mom_panel.rank(axis=1, ascending=False)

    for sym, df in ctx.bars_by_tv.items():
        # Signal is 1 if it is the #1 ranked asset
        df["signal"] = (ranks[sym] == 1).astype(int)

    return ctx.bars_by_tv


@strategy(
    "qc_momentum_in_mutual_fund_returns",
    entry="signal == 1",
    exit="signal == 0",
    prepare_bars=prepare_mf_momentum,
    required_lookback=lambda: 252,
)
def _qc_momentum_in_mutual_fund_returns():
    pass
