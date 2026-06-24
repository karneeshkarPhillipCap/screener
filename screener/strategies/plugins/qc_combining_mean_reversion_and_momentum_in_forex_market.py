import numpy as np
import pandas as pd
from screener.strategies.spec import strategy, PrepareCtx


def prepare_forex_mom_mr(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    """
    Calculate momentum and mean reversion factors, then rank across symbols.
    Original strategy used OLS weights, we approximate by equal-weighting
    normalized momentum and normalized reversal.
    """
    for sym, df in ctx.bars_by_tv.items():
        if len(df) > 756:
            # 3-month momentum (approx 63 trading days)
            mom = df["close"] - df["close"].shift(63)
            # Long-term mean and std (approx 3 years = 756 days)
            roll_mean = df["close"].rolling(756).mean()
            roll_std = df["close"].rolling(756).std()

            # Reversal factor: higher price relative to mean = stronger mean reversion downward
            reversal = (df["close"] - roll_mean) / roll_std

            # Normalize momentum to match scale of reversal
            mom_mean = mom.rolling(756).mean()
            mom_std = mom.rolling(756).std()
            # Avoid division by zero
            mom_std = mom_std.replace(0, np.nan)
            mom_z = (mom - mom_mean) / mom_std

            # Expected return proxy: high momentum, low price (high negative reversal)
            df["score"] = mom_z - reversal
        else:
            df["score"] = np.nan

    score_panel = pd.DataFrame({sym: df["score"] for sym, df in ctx.bars_by_tv.items()})

    # Rank daily across symbols. Highest score -> rank 1
    ranks = score_panel.rank(axis=1, ascending=False)

    for sym, df in ctx.bars_by_tv.items():
        # Long highest expected return if it's positive
        # Original strategy would short the worst if negative, but framework is long-only
        sym_score = df.get("score", pd.Series(np.nan, index=df.index))
        sym_rank = (
            ranks[sym] if sym in ranks.columns else pd.Series(np.nan, index=df.index)
        )

        df["signal"] = ((sym_rank == 1) & (sym_score > 0)).astype(int)

    return ctx.bars_by_tv


@strategy(
    "qc_combining_mean_reversion_and_momentum_in_forex_market",
    entry="signal == 1",
    exit="signal == 0",
    prepare_bars=prepare_forex_mom_mr,
    required_lookback=lambda: 756,
)
def _qc_combining_mean_reversion_and_momentum_in_forex_market():
    pass
