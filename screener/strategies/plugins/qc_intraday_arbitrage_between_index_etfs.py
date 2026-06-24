import pandas as pd
from screener.strategies.spec import PrepareCtx, strategy


def _prepare_qc_intraday_arbitrage_between_index_etfs(
    ctx: PrepareCtx,
) -> dict[str, pd.DataFrame]:
    prepared: dict[str, pd.DataFrame] = {}

    # Calculate cross-sectional mean (market proxy)
    closes: dict[str, pd.Series] = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if bars is not None and not bars.empty:
            closes[symbol] = bars["close"]

    if not closes:
        return prepared

    market_proxy = pd.DataFrame(closes).mean(axis=1)

    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue

        df = bars.copy().sort_index()

        # Approximate pairs spread using ratio against cross-sectional mean
        ratio = df["close"] / market_proxy
        rolling_mean = ratio.rolling(window=400).mean()
        rolling_std = ratio.rolling(window=400).std()

        z_score = (ratio - rolling_mean) / (rolling_std + 1e-9)

        # Entry when z_score drops below -2.0 (symbol is undervalued relative to mean)
        df["entry_signal"] = z_score < -2.0
        # Exit when z_score reverts to mean
        df["exit_signal"] = z_score > 0.0

        prepared[symbol] = df

    return prepared


def _lookback_qc_intraday_arbitrage_between_index_etfs() -> int:
    return 400


@strategy(
    "qc_intraday_arbitrage_between_index_etfs",
    entry="entry_signal",
    exit="exit_signal",
    prepare_bars=_prepare_qc_intraday_arbitrage_between_index_etfs,
    required_lookback=_lookback_qc_intraday_arbitrage_between_index_etfs,
)
def _qc_intraday_arbitrage_between_index_etfs() -> None:
    pass
