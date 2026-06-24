import pandas as pd
from screener.strategies.spec import PrepareCtx, strategy


def _prepare_qc_gaussian_naive_bayes_model(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    prepared = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue
        df = bars.copy().sort_index()

        # Approximate the ML model using the 4-day momentum of the open-to-close return
        open_close_ret = (df["close"] - df["open"]) / df["open"]
        momentum = open_close_ret.rolling(4).sum()

        df["entry_signal"] = momentum > 0
        df["exit_signal"] = momentum < 0
        prepared[symbol] = df
    return prepared


def _lookback_qc_gaussian_naive_bayes_model() -> int:
    return 4


@strategy(
    "qc_gaussian_naive_bayes_model",
    entry="entry_signal",
    exit="exit_signal",
    prepare_bars=_prepare_qc_gaussian_naive_bayes_model,
    required_lookback=_lookback_qc_gaussian_naive_bayes_model,
)
def _qc_gaussian_naive_bayes_model() -> None:
    pass
