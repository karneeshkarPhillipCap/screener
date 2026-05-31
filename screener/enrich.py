"""Fundamental enrichment (P/E, ROCE, ROE).

Financial Modeling Prep is the primary source and works for US and Indian
symbols. The legacy openscreener scrape is retained as an India-only fallback
for when FMP is unavailable (no API key or request failure).
"""

from __future__ import annotations

import pandas as pd

from screener.providers.fmp import FmpApiKeyError, FmpClient
from screener.providers.fmp_fundamentals import fetch_fundamentals


def enrich_fundamentals(df: pd.DataFrame, market: str) -> pd.DataFrame:
    if df is None or df.empty or "name" not in df.columns:
        return df

    symbols = [s for s in df["name"].tolist() if s]
    if not symbols:
        return df

    data = _fmp_fundamentals(symbols, market)
    if not data and market == "india":
        data = _openscreener_fundamentals(symbols)
    if not data:
        return df

    fundamentals = pd.DataFrame([{"name": symbol, **vals} for symbol, vals in data.items()])
    return df.merge(fundamentals, on="name", how="left")


def _fmp_fundamentals(symbols: list[str], market: str) -> dict[str, dict] | None:
    """Return ``{symbol: {P/E, ROCE%, ROE%}}`` from FMP, or None if unavailable."""
    try:
        client = FmpClient()
    except FmpApiKeyError:
        return None
    try:
        return fetch_fundamentals(symbols, market=market, client=client)
    except Exception:
        return None


def _openscreener_fundamentals(symbols: list[str]) -> dict[str, dict] | None:
    """Legacy India-only fallback via the openscreener scrape."""
    try:
        from openscreener import Stock
    except ImportError:
        return None

    try:
        batch = Stock.batch(symbols)
        ratios_data = batch.fetch("ratios")
    except (AttributeError, RuntimeError, ConnectionError, TimeoutError):
        return None

    data: dict[str, dict] = {}
    for symbol in symbols:
        row = ratios_data.get(symbol, {})
        data[symbol] = {
            "P/E": row.get("stock_p_e"),
            "ROCE%": row.get("roce_percent"),
            "ROE%": row.get("return_on_equity"),
        }
    return data
