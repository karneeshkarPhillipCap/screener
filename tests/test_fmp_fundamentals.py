from __future__ import annotations

import pandas as pd

from screener.enrich import enrich_fundamentals
from screener.providers.fmp import FmpClient
from screener.providers.fmp_fundamentals import fetch_fundamentals


class _Resp:
    def __init__(self, payload: object) -> None:
        self.payload = payload
        self.status_code = 200
        self.text = ""
        self.headers: dict[str, str] = {}

    def json(self) -> object:
        return self.payload


class _RoutingSession:
    """Dispatch GET by endpoint substring; record (url, params) calls."""

    def __init__(self, routes: dict[str, object]) -> None:
        self.routes = routes
        self.calls: list[tuple[str, dict[str, object]]] = []

    def get(self, url: str, *, params=None, timeout=None):
        self.calls.append((url, dict(params or {})))
        for sub, payload in self.routes.items():
            if sub in url:
                return _Resp(payload)
        return _Resp([])


def _client(routes: dict[str, object]) -> tuple[FmpClient, _RoutingSession]:
    session = _RoutingSession(routes)
    return FmpClient(api_key="k", session=session, load_env=False), session


def test_fetch_fundamentals_scales_percentages():
    client, _ = _client(
        {
            "key-metrics-ttm": [
                {"returnOnEquityTTM": 0.15, "returnOnCapitalEmployedTTM": 0.22}
            ],
            "ratios-ttm": [{"priceToEarningsRatioTTM": 18.5}],
        }
    )
    data = fetch_fundamentals(["AAPL"], market="us", client=client, cache_ttl=None)
    assert data["AAPL"] == {"P/E": 18.5, "ROCE%": 22.0, "ROE%": 15.0}


def test_fetch_fundamentals_appends_ns_for_india():
    client, session = _client(
        {
            "key-metrics-ttm": [
                {"returnOnEquityTTM": 0.1, "returnOnCapitalEmployedTTM": 0.2}
            ],
            "ratios-ttm": [{"priceToEarningsRatioTTM": 25.0}],
        }
    )
    fetch_fundamentals(["RELIANCE"], market="india", client=client, cache_ttl=None)
    assert any(params.get("symbol") == "RELIANCE.NS" for _, params in session.calls)


def test_fetch_fundamentals_omits_symbols_without_data():
    client, _ = _client({"key-metrics-ttm": [], "ratios-ttm": []})
    data = fetch_fundamentals(["NOPE"], market="us", client=client, cache_ttl=None)
    assert data == {}


def test_enrich_fundamentals_merges_fmp_columns(monkeypatch):
    df = pd.DataFrame({"name": ["AAPL", "MSFT"], "close": [1.0, 2.0]})
    monkeypatch.setattr(
        "screener.enrich._fmp_fundamentals",
        lambda symbols, market: {"AAPL": {"P/E": 18.5, "ROCE%": 22.0, "ROE%": 15.0}},
    )
    out = enrich_fundamentals(df, "us")
    assert list(out.columns) == ["name", "close", "P/E", "ROCE%", "ROE%"]
    indexed = out.set_index("name")
    assert indexed.loc["AAPL", "P/E"] == 18.5
    assert indexed.loc["AAPL", "ROE%"] == 15.0
    assert pd.isna(indexed.loc["MSFT", "P/E"])


def test_enrich_falls_back_to_openscreener_for_india(monkeypatch):
    df = pd.DataFrame({"name": ["RELIANCE"]})
    monkeypatch.setattr("screener.enrich._fmp_fundamentals", lambda symbols, market: None)
    monkeypatch.setattr(
        "screener.enrich._openscreener_fundamentals",
        lambda symbols: {"RELIANCE": {"P/E": 30.0, "ROCE%": 18.0, "ROE%": 12.0}},
    )
    out = enrich_fundamentals(df, "india")
    assert out.set_index("name").loc["RELIANCE", "ROCE%"] == 18.0


def test_enrich_does_not_fall_back_for_us(monkeypatch):
    df = pd.DataFrame({"name": ["AAPL"]})
    monkeypatch.setattr("screener.enrich._fmp_fundamentals", lambda symbols, market: None)

    def _boom(symbols):  # pragma: no cover - must not be called for US
        raise AssertionError("openscreener fallback should be India-only")

    monkeypatch.setattr("screener.enrich._openscreener_fundamentals", _boom)
    out = enrich_fundamentals(df, "us")
    assert list(out.columns) == ["name"]
