"""Fundamental ratios (P/E, ROCE, ROE) from Financial Modeling Prep.

FMP serves these for US and Indian symbols alike, so this is the primary
fundamentals source. ROE/ROCE come back as decimals (0.15) and are scaled to
percent here to match the screener's display columns.
"""

from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor, as_completed
from typing import Any, Iterable, Mapping

from screener.providers.fmp import FmpClient

FUNDAMENTAL_COLUMNS = ("P/E", "ROCE%", "ROE%")
_DEFAULT_TTL = 86_400.0  # cache fundamentals for one day


def _fmp_symbol(symbol: str, market: str) -> str:
    """Map a bare screener symbol to the FMP ticker form (NSE for India)."""
    if market == "india" and "." not in symbol and ":" not in symbol:
        return f"{symbol}.NS"
    return symbol


def _num(value: Any) -> float | None:
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def _pct(value: Any) -> float | None:
    """FMP returns ROE/ROCE as decimals (0.15); display expects percent."""
    parsed = _num(value)
    return parsed * 100.0 if parsed is not None else None


def _first_row(payload: Any) -> Mapping[str, Any]:
    if isinstance(payload, list):
        return payload[0] if payload and isinstance(payload[0], Mapping) else {}
    if isinstance(payload, Mapping):
        return payload
    return {}


def _fetch_one(
    client: FmpClient,
    fmp_symbol: str,
    *,
    cache_ttl: float | None,
    refresh: bool,
) -> dict[str, float | None] | None:
    metrics = _first_row(
        client.get_json(
            "key-metrics-ttm",
            params={"symbol": fmp_symbol},
            cache_ttl=cache_ttl,
            refresh=refresh,
            fallback=None,
        )
    )
    ratios = _first_row(
        client.get_json(
            "ratios-ttm",
            params={"symbol": fmp_symbol},
            cache_ttl=cache_ttl,
            refresh=refresh,
            fallback=None,
        )
    )
    if not metrics and not ratios:
        return None
    return {
        "P/E": _num(ratios.get("priceToEarningsRatioTTM")),
        "ROCE%": _pct(metrics.get("returnOnCapitalEmployedTTM")),
        "ROE%": _pct(metrics.get("returnOnEquityTTM")),
    }


def fetch_fundamentals(
    symbols: Iterable[str],
    *,
    market: str,
    client: FmpClient | None = None,
    cache_ttl: float | None = _DEFAULT_TTL,
    refresh: bool = False,
    max_workers: int = 8,
) -> dict[str, dict[str, float | None]]:
    """Fetch ``{symbol: {P/E, ROCE%, ROE%}}`` from FMP.

    Symbols that error or return no data are omitted. Raises
    :class:`~screener.providers.fmp.FmpApiKeyError` only when no ``client`` is
    supplied and no API key is configured.
    """
    unique = [s for s in dict.fromkeys(symbols) if s]
    if not unique:
        return {}
    client = client or FmpClient()
    out: dict[str, dict[str, float | None]] = {}
    workers = max(1, min(max_workers, len(unique)))
    with ThreadPoolExecutor(max_workers=workers) as pool:
        futures = {
            pool.submit(
                _fetch_one,
                client,
                _fmp_symbol(symbol, market),
                cache_ttl=cache_ttl,
                refresh=refresh,
            ): symbol
            for symbol in unique
        }
        for future in as_completed(futures):
            symbol = futures[future]
            try:
                row = future.result()
            except Exception:
                row = None
            if row is not None:
                out[symbol] = row
    return out
