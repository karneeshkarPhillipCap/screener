"""Current index constituent loaders used by rolling backtests."""

from __future__ import annotations

from datetime import date
import hashlib
import json
from pathlib import Path
from typing import Literal, Mapping

import pandas as pd
from pydantic import BaseModel, ConfigDict, field_validator
import requests

from screener.providers.fmp import FmpClient
from screener.providers.fmp_screener import SUPPORTED_PARAMS, screen_symbols
from screener.resilience import call_with_resilience


CACHE_DIR = Path.home() / ".screener" / "universes"
UniverseName = Literal["sp500", "nifty50"]


class Universe(BaseModel):
    name: str
    symbols: tuple[str, ...]
    source: str
    cached_path: Path

    model_config = ConfigDict(frozen=True)

    @field_validator("symbols")
    @classmethod
    def _validate_symbols(cls, value: tuple[str, ...]) -> tuple[str, ...]:
        normalized = tuple(symbol.strip() for symbol in value if symbol.strip())
        if not normalized:
            raise ValueError("symbols must not be empty")
        return normalized

    @field_validator("source")
    @classmethod
    def _normalize_source(cls, value: str) -> str:
        normalized = value.strip()
        if not normalized:
            raise ValueError("source must not be empty")
        return normalized


def _cache_path(name: str, as_of: date) -> Path:
    return CACHE_DIR / f"{name}_{as_of.isoformat()}.txt"


def _write_cache(name: str, as_of: date, symbols: list[str], source: str) -> Path:
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    path = _cache_path(name, as_of)
    lines = [
        f"# universe={name}",
        f"# as_of={as_of.isoformat()}",
        f"# source={source}",
        *symbols,
    ]
    path.write_text("\n".join(lines) + "\n")
    return path


def _read_cache(name: str, as_of: date) -> Universe | None:
    path = _cache_path(name, as_of)
    if not path.exists():
        return None
    source = "cache"
    symbols: list[str] = []
    for line in path.read_text().splitlines():
        line = line.strip()
        if not line:
            continue
        if line.startswith("# source="):
            source = line.split("=", 1)[1]
            continue
        if line.startswith("#"):
            continue
        symbols.append(line)
    if not symbols:
        return None
    return Universe(name=name, symbols=tuple(symbols), source=source, cached_path=path)


def load_current_universe(
    name: UniverseName,
    *,
    as_of: date | None = None,
    use_cache: bool = True,
) -> Universe:
    as_of = as_of or date.today()
    if use_cache:
        cached = _read_cache(name, as_of)
        if cached is not None:
            return cached
    if name == "sp500":
        symbols, source = _fetch_sp500()
    elif name == "nifty50":
        symbols, source = _fetch_nifty50()
    else:
        raise ValueError(f"unknown universe: {name}")
    path = _write_cache(name, as_of, symbols, source)
    return Universe(name=name, symbols=tuple(symbols), source=source, cached_path=path)


def build_fmp_universe(
    *,
    filters: Mapping[str, object],
    base: UniverseName | None = None,
    market: str = "us",
    as_of: date | None = None,
    use_cache: bool = True,
    client: FmpClient | None = None,
    limit: int | None = None,
    refresh: bool = False,
) -> Universe:
    """Build a dynamic universe from FMP's company-screener endpoint."""
    as_of = as_of or date.today()
    name = _fmp_cache_name(filters, base=base, market=market, limit=limit)
    if use_cache:
        cached = _read_cache(name, as_of)
        if cached is not None:
            return cached
    rows = screen_symbols(
        filters, client=client, limit=limit, refresh=refresh or not use_cache
    )
    symbols = _dedupe([_normalize_fmp_symbol(row.symbol, market) for row in rows])
    source = "fmp:company-screener"
    if base is not None:
        base_universe = load_current_universe(base, as_of=as_of, use_cache=use_cache)
        base_symbols = set(base_universe.symbols)
        symbols = [symbol for symbol in symbols if symbol in base_symbols]
        source = f"{source}; base={base}"
    if not symbols:
        raise RuntimeError("FMP universe resolved to no symbols")
    path = _write_cache(name, as_of, symbols, source)
    return Universe(name=name, symbols=tuple(symbols), source=source, cached_path=path)


def _fmp_cache_name(
    filters: Mapping[str, object],
    *,
    base: UniverseName | None,
    market: str,
    limit: int | None,
) -> str:
    supported_filters = {
        key: value
        for key, value in sorted(filters.items())
        if key in SUPPORTED_PARAMS and value is not None
    }
    if limit is not None:
        supported_filters["limit"] = int(limit)
    payload = {
        "base": base,
        "filters": supported_filters,
        "market": market,
    }
    raw = json.dumps(payload, default=str, sort_keys=True, separators=(",", ":"))
    digest = hashlib.sha256(raw.encode("utf-8")).hexdigest()[:16]
    return f"fmp_{digest}"


def _normalize_fmp_symbol(symbol: str, market: str) -> str:
    normalized = symbol.strip().upper()
    if market == "us":
        normalized = normalized.replace(".", "-")
    return normalized


def _fetch_sp500() -> tuple[list[str], str]:
    source = "https://en.wikipedia.org/wiki/List_of_S%26P_500_companies"
    headers = {
        "User-Agent": (
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 "
            "KHTML, like Gecko) Chrome/122.0 Safari/537.36"
        )
    }
    resp = call_with_resilience(
        "wikipedia",
        "sp500 constituents",
        lambda: requests.get(source, headers=headers, timeout=30),
        fallback=None,
    )
    if resp is None:
        raise RuntimeError("S&P 500 constituents unavailable")
    resp.raise_for_status()
    from io import StringIO

    tables = pd.read_html(StringIO(resp.text))
    if not tables:
        raise RuntimeError("S&P 500 constituents table not found")
    df = tables[0]
    if "Symbol" not in df.columns:
        raise RuntimeError("S&P 500 constituents table missing Symbol column")
    symbols = (
        df["Symbol"]
        .dropna()
        .astype(str)
        .str.strip()
        .str.upper()
        .str.replace(".", "-", regex=False)
        .tolist()
    )
    return _dedupe(symbols), source


def _fetch_nifty50() -> tuple[list[str], str]:
    source = "https://archives.nseindia.com/content/indices/ind_nifty50list.csv"
    headers = {
        "User-Agent": (
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 "
            "KHTML, like Gecko) Chrome/122.0 Safari/537.36"
        )
    }
    resp = call_with_resilience(
        "nse",
        "nifty50 constituents",
        lambda: requests.get(source, headers=headers, timeout=30),
        fallback=None,
    )
    if resp is None:
        raise RuntimeError("Nifty 50 constituents unavailable")
    resp.raise_for_status()
    from io import StringIO

    df = pd.read_csv(StringIO(resp.text))
    symbol_col = "Symbol" if "Symbol" in df.columns else "SYMBOL"
    if symbol_col not in df.columns:
        raise RuntimeError("Nifty 50 constituents CSV missing Symbol column")
    symbols = df[symbol_col].dropna().astype(str).str.strip().str.upper().tolist()
    return _dedupe(symbols), source


def _dedupe(symbols: list[str]) -> list[str]:
    return list(dict.fromkeys(s for s in symbols if s))
