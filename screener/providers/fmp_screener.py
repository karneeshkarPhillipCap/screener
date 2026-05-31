"""Company screener rows from Financial Modeling Prep."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Mapping

from screener.providers.fmp import FmpClient

_DEFAULT_TTL = 86_400.0

SUPPORTED_PARAMS = frozenset(
    {
        "marketCapMoreThan",
        "marketCapLowerThan",
        "priceMoreThan",
        "priceLowerThan",
        "betaMoreThan",
        "betaLowerThan",
        "volumeMoreThan",
        "volumeLowerThan",
        "dividendMoreThan",
        "dividendLowerThan",
        "sector",
        "industry",
        "exchange",
        "country",
        "isEtf",
        "isFund",
        "isActivelyTrading",
        "limit",
    }
)


@dataclass(frozen=True)
class ScreenerRow:
    symbol: str
    company_name: str | None = None
    market_cap: float | None = None
    price: float | None = None
    beta: float | None = None
    volume: float | None = None
    last_annual_dividend: float | None = None
    exchange: str | None = None
    exchange_short_name: str | None = None
    sector: str | None = None
    industry: str | None = None
    country: str | None = None
    is_etf: bool | None = None
    is_fund: bool | None = None
    is_actively_trading: bool | None = None


def _num(value: Any) -> float | None:
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def _str(value: Any) -> str | None:
    if value is None:
        return None
    text = str(value).strip()
    return text or None


def _bool(value: Any) -> bool | None:
    if value is None:
        return None
    if isinstance(value, bool):
        return value
    if isinstance(value, str):
        normalized = value.strip().lower()
        if normalized in {"true", "1", "yes"}:
            return True
        if normalized in {"false", "0", "no"}:
            return False
    return bool(value)


def _filters(filters: Mapping[str, object], limit: int | None) -> dict[str, object]:
    params = {
        key: value
        for key, value in filters.items()
        if key in SUPPORTED_PARAMS and value is not None
    }
    if limit is not None:
        params["limit"] = int(limit)
    return params


def _row(payload: Mapping[str, Any]) -> ScreenerRow | None:
    symbol = _str(payload.get("symbol"))
    if symbol is None:
        return None
    return ScreenerRow(
        symbol=symbol,
        company_name=_str(payload.get("companyName")),
        market_cap=_num(payload.get("marketCap")),
        price=_num(payload.get("price")),
        beta=_num(payload.get("beta")),
        volume=_num(payload.get("volume")),
        last_annual_dividend=_num(payload.get("lastAnnualDividend")),
        exchange=_str(payload.get("exchange")),
        exchange_short_name=_str(payload.get("exchangeShortName")),
        sector=_str(payload.get("sector")),
        industry=_str(payload.get("industry")),
        country=_str(payload.get("country")),
        is_etf=_bool(payload.get("isEtf")),
        is_fund=_bool(payload.get("isFund")),
        is_actively_trading=_bool(payload.get("isActivelyTrading")),
    )


def screen_symbols(
    filters: Mapping[str, object],
    *,
    client: FmpClient | None = None,
    limit: int | None = None,
    cache_ttl: float | None = _DEFAULT_TTL,
    refresh: bool = False,
) -> list[ScreenerRow]:
    """Return FMP company-screener rows for supported filters."""
    client = client or FmpClient()
    payload = client.get_json(
        "company-screener",
        params=_filters(filters, limit),
        cache_ttl=cache_ttl,
        refresh=refresh,
        fallback=None,
    )
    if payload is None:
        raise RuntimeError("FMP company screener unavailable")
    if not isinstance(payload, list):
        raise RuntimeError("FMP company screener returned an invalid payload")
    rows: list[ScreenerRow] = []
    for item in payload:
        if not isinstance(item, Mapping):
            continue
        row = _row(item)
        if row is not None:
            rows.append(row)
    return rows
