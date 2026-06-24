import pandas as pd
from screener.strategies.spec import PrepareCtx, strategy


def _prepare_qc_gradient_boosting_model(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    prepared = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue
        df = bars.copy().sort_index()

        # Approximate GBM using 10-period momentum (ROC)
        # Original logic requires online ML training; we substitute with heuristic.
        roc = df["close"].pct_change(periods=10)

        df["entry_signal"] = roc > 0.0005
        df["exit_signal"] = roc < -0.0005

        prepared[symbol] = df
    return prepared


def _lookback_qc_gradient_boosting_model() -> int:
    # 10 periods needed for the momentum ROC calculation
    return 10


@strategy(
    "qc_gradient_boosting_model",
    entry="entry_signal",
    exit="exit_signal",
    prepare_bars=_prepare_qc_gradient_boosting_model,
    required_lookback=_lookback_qc_gradient_boosting_model,
)
def _qc_gradient_boosting_model() -> None:
    pass
