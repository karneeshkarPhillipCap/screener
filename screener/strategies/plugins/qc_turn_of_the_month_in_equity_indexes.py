import pandas as pd

from screener.strategies.spec import PrepareCtx, strategy


def _prepare_totm(ctx: PrepareCtx) -> dict[str, pd.DataFrame]:
    prepared = {}

    for sym, bars in ctx.bars_by_tv.items():
        if bars.empty:
            prepared[sym] = bars
            continue

        df = bars.copy().sort_index()

        # Identify the last trading day of the month
        month = df.index.month
        # The next day's month is different than today's month -> today is trading month end
        is_trading_month_end = month != pd.Series(month).shift(-1).values
        # The last row of the dataset is also technically the last known day, but we can't be sure it's month end.
        is_trading_month_end[-1] = False

        # We want to be invested on:
        # 1. The last trading day of the month
        # 2. The 1st trading day of the next month
        # 3. The 2nd trading day of the next month
        # 4. The 3rd trading day of the next month
        # So we can just take a rolling window of max over the last 4 days.
        # Wait, if is_trading_month_end is True at T, we want entry to be True at T, T+1, T+2, T+3.
        # We can achieve this by rolling sum of is_trading_month_end over a window of 4, looking backwards?
        # A backward shift of is_trading_month_end:
        # T (month end): is_trading_month_end = True
        # T+1: shifted by 1 = True
        # T+2: shifted by 2 = True
        # T+3: shifted by 3 = True

        s_end = pd.Series(is_trading_month_end, index=df.index)

        entry = (
            s_end
            | s_end.shift(1, fill_value=False)
            | s_end.shift(2, fill_value=False)
            | s_end.shift(3, fill_value=False)
        )

        df["totm_entry"] = entry
        df["totm_exit"] = ~entry
        prepared[sym] = df

    return prepared


@strategy(
    "qc_turn_of_the_month_in_equity_indexes",
    entry="totm_entry",
    exit="totm_exit",
    prepare_bars=_prepare_totm,
)
def _totm() -> None:
    """
    Turn of the Month in Equity Indexes.
    Buys SPY on the last trading day of the month and holds it
    for the first 3 trading days of the next month.
    """
    pass
