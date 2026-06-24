import pandas as pd
import numpy as np
from screener.strategies.spec import strategy, PrepareCtx
from itertools import combinations


def prepare_pairs(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    # Need at least 252 days for formation
    # We will use the first 252 days to form pairs

    # Filter symbols that have enough data
    valid_symbols = [sym for sym, df in ctx.bars_by_tv.items() if len(df) > 252]

    if len(valid_symbols) < 2:
        for sym, df in ctx.bars_by_tv.items():
            df["signal"] = 0
        return ctx.bars_by_tv

    # Extract first 252 days of prices
    prices = {}
    for sym in valid_symbols:
        df = ctx.bars_by_tv[sym]
        prices[sym] = df["close"].iloc[:252].values

    # Make sure they are the same length (some might be shorter if they IPO'd late,
    # but we took first 252. We should align by date, but since index is date,
    # let's just use the aligned panel).

    panel = pd.DataFrame({sym: ctx.bars_by_tv[sym]["close"] for sym in valid_symbols})
    formation_panel = panel.iloc[:252].dropna(axis=1)

    active_symbols = list(formation_panel.columns)
    if len(active_symbols) < 2:
        for sym, df in ctx.bars_by_tv.items():
            df["signal"] = 0
        return ctx.bars_by_tv

    # Normalize prices (divide by first price)
    norm_prices = formation_panel.div(formation_panel.iloc[0])

    distances = []
    # To avoid N^2 taking too long if universe is 500 (125k pairs),
    # we limit the universe to the first 100 for pair selection if it's too large.
    eval_symbols = active_symbols[:100]

    for s1, s2 in combinations(eval_symbols, 2):
        dist = np.sum((norm_prices[s1] - norm_prices[s2]) ** 2)
        distances.append((dist, s1, s2))

    distances.sort(key=lambda x: x[0])
    top_4_pairs = distances[:4]

    # Initialize signals to 0
    for sym, df in ctx.bars_by_tv.items():
        df["signal"] = 0

    # For each pair, compute the rolling spread and generate signals
    for _, s1, s2 in top_4_pairs:
        p1 = panel[s1]
        p2 = panel[s2]

        # We normalize prices continuously for the spread? The spec says spread of prices.
        # Usually it's spread = log(p1) - log(p2) or p1 - hedge_ratio * p2.
        # We will use simple normalized difference spread.
        # Re-normalize to rolling 252 days or just use log spread. Log spread is standard.
        spread = np.log(p1) - np.log(p2)

        # 1-year rolling mean and std
        roll_mean = spread.rolling(252).mean()
        roll_std = spread.rolling(252).std()

        z_score = (spread - roll_mean) / roll_std

        # Generate signals
        # If z_score > 2: spread is high (p1 > p2), so short p1, long p2
        # If z_score < -2: spread is low (p1 < p2), so long p1, short p2
        # If abs(z_score) < 0.5: close positions

        signal1 = pd.Series(0, index=p1.index)
        signal2 = pd.Series(0, index=p2.index)

        # This is a simplified vectorized state machine
        signal1[z_score > 2] = -1
        signal2[z_score > 2] = 1

        signal1[z_score < -2] = 1
        signal2[z_score < -2] = -1

        # It doesn't hold the state perfectly but it generates entry/exit signals.
        # For a full backtest, we would ffill the position until mean reversion.
        position1 = signal1.replace(0, np.nan)
        position1[np.abs(z_score) < 0.5] = 0
        position1 = position1.ffill().fillna(0)

        position2 = signal2.replace(0, np.nan)
        position2[np.abs(z_score) < 0.5] = 0
        position2 = position2.ffill().fillna(0)

        ctx.bars_by_tv[s1]["signal"] = position1
        ctx.bars_by_tv[s2]["signal"] = position2

    return ctx.bars_by_tv


@strategy(
    "qc_pairs-trading-with-stocks",
    entry="signal != 0",
    exit="signal == 0",
    prepare_bars=prepare_pairs,
    required_lookback=lambda: 252,
)
def _qc_pairs_trading():
    pass
