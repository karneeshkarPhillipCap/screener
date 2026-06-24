import pandas as pd
from screener.strategies.spec import PrepareCtx, strategy


def _prepare_qc_using_news_sentiment_to_predict_price_direction_of_drug_manufacturers(
    ctx: PrepareCtx,
) -> dict[str, pd.DataFrame]:
    prepared = {}
    for symbol, bars in ctx.bars_by_tv.items():
        if bars is None or bars.empty:
            prepared[symbol] = bars
            continue
        df = bars.copy().sort_index()

        # Approximate NLP sentiment using 5-day price momentum
        momentum = df["close"].pct_change(5)

        # The strategy trades exclusively on Wednesdays, holding intraday.
        # In a daily framework, we enter on Tuesday close and exit on Wednesday close.
        # dayofweek: Monday=0, Tuesday=1, Wednesday=2
        is_tuesday = df.index.dayofweek == 1
        is_wednesday = df.index.dayofweek == 2

        df["entry_signal"] = is_tuesday & (momentum > 0)
        df["exit_signal"] = is_wednesday
        prepared[symbol] = df
    return prepared


def _lookback_qc_using_news_sentiment_to_predict_price_direction_of_drug_manufacturers() -> (
    int
):
    return 5


@strategy(
    "qc_using_news_sentiment_to_predict_price_direction_of_drug_manufacturers",
    entry="entry_signal",
    exit="exit_signal",
    prepare_bars=_prepare_qc_using_news_sentiment_to_predict_price_direction_of_drug_manufacturers,
    required_lookback=_lookback_qc_using_news_sentiment_to_predict_price_direction_of_drug_manufacturers,
)
def _qc_using_news_sentiment_to_predict_price_direction_of_drug_manufacturers() -> None:
    pass
