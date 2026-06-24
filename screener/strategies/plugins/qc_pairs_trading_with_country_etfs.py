import pandas as pd
import numpy as np
from itertools import combinations

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_pairs_trading(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    # Collect closes
    closes = {}
    for sym, bars in ctx.bars_by_tv.items():
        if not bars.empty:
            closes[sym] = bars["close"].astype(float)

    if not closes:
        return ctx.bars_by_tv

    df_close = pd.DataFrame(closes).sort_index()

    entry_signals = pd.DataFrame(False, index=df_close.index, columns=df_close.columns)
    exit_signals = pd.DataFrame(False, index=df_close.index, columns=df_close.columns)

    symbols = list(df_close.columns)
    active_pairs = {}

    for i in range(120, len(df_close)):
        current_date = df_close.index[i]
        prev_date = df_close.index[i - 1]

        # Rebalance at the start of a new month or first time
        if current_date.month != prev_date.month or i == 120:
            window = df_close.iloc[i - 120 : i]
            # normalized using bfill to prevent NaNs at start
            norm_prices = window / window.bfill().iloc[0]

            pair_distances = []
            for s1, s2 in combinations(symbols, 2):
                if pd.isna(norm_prices[s1].iloc[-1]) or pd.isna(
                    norm_prices[s2].iloc[-1]
                ):
                    continue
                dist = np.mean(np.abs(norm_prices[s1] - norm_prices[s2]))
                pair_distances.append((dist, s1, s2))

            pair_distances.sort(key=lambda x: x[0])
            top_5 = pair_distances[:5]

            active_pairs = {(p[1], p[2]): p[0] for p in top_5}

        if not active_pairs:
            continue

        window_today = df_close.iloc[i - 119 : i + 1]
        norm_today = window_today.iloc[-1] / window_today.bfill().iloc[0]

        for (s1, s2), dist in active_pairs.items():
            idx1 = norm_today.get(s1, np.nan)
            idx2 = norm_today.get(s2, np.nan)

            if pd.isna(idx1) or pd.isna(idx2):
                continue

            diff = idx1 - idx2

            # Approximation: Only trade the long leg of the pair.
            if diff > 0.5 * dist:
                # s1 outperformed s2, so long s2
                entry_signals.at[current_date, s2] = True
            elif diff < -0.5 * dist:
                # s2 outperformed s1, so long s1
                entry_signals.at[current_date, s1] = True
            elif abs(diff) <= 0.5 * dist:
                exit_signals.at[current_date, s1] = True
                exit_signals.at[current_date, s2] = True

    prepared = {}
    for sym, bars in ctx.bars_by_tv.items():
        if bars.empty:
            prepared[sym] = bars
            continue

        df = bars.copy()
        df["pairs_entry"] = entry_signals[sym].reindex(df.index).fillna(False)
        df["pairs_exit"] = exit_signals[sym].reindex(df.index).fillna(False)
        prepared[sym] = df

    return prepared


def _pairs_trading_lookback() -> int:
    return 120


@strategy(
    "qc_pairs_trading_with_country_etfs",
    entry="pairs_entry",
    exit="pairs_exit",
    prepare_bars=_prepare_pairs_trading,
    required_lookback=_pairs_trading_lookback,
)
def _pairs_trading() -> None:
    """
    Pairs Trading with Country ETFs.
    Approximation: Long-only implementation of pairs trading due to
    framework constraints. We identify the top 5 cointegrated pairs
    monthly, and take a long position in the underperforming asset
    when the distance exceeds the threshold.
    """
    pass
