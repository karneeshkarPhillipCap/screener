import pandas as pd
from screener.strategies.spec import PrepareCtx, strategy


def _prepare_qc_g_score_investing(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    prepared = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue
        df = bars.copy().sort_index()

        # Approximate G-Score using technical indicators since fundamental data is unavailable.
        sma_50 = df["close"].rolling(window=50).mean()
        sma_200 = df["close"].rolling(window=200).mean()
        std_20 = df["close"].rolling(window=20).std()
        std_50 = df["close"].rolling(window=50).std()

        score1 = (df["close"] > sma_200).astype(int)
        score2 = (df["close"] > sma_50).astype(int)
        score3 = (std_20 < std_50).astype(int)

        total_score = score1 + score2 + score3

        df["entry_signal"] = total_score >= 2
        df["exit_signal"] = total_score < 2

        prepared[symbol] = df
    return prepared


def _lookback_qc_g_score_investing() -> int:
    return 200


@strategy(
    "qc_g_score_investing",
    entry="entry_signal",
    exit="exit_signal",
    prepare_bars=_prepare_qc_g_score_investing,
    required_lookback=_lookback_qc_g_score_investing,
)
def _qc_g_score_investing() -> None:
    pass
