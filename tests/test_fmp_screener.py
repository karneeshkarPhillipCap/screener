from __future__ import annotations

from screener.providers.fmp import FmpClient
from screener.providers.fmp_screener import screen_symbols


class _Resp:
    def __init__(self, payload: object) -> None:
        self.payload = payload
        self.status_code = 200
        self.text = ""
        self.headers: dict[str, str] = {}

    def json(self) -> object:
        return self.payload


class _RoutingSession:
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


def test_screen_symbols_passes_supported_filters_and_drops_none():
    client, session = _client(
        {
            "company-screener": [
                {
                    "symbol": "AAPL",
                    "companyName": "Apple Inc.",
                    "marketCap": 1000,
                    "price": 10.5,
                    "isEtf": False,
                    "isFund": False,
                    "isActivelyTrading": True,
                }
            ]
        }
    )

    rows = screen_symbols(
        {
            "marketCapMoreThan": 1_000_000,
            "sector": "Technology",
            "country": None,
            "unsupported": "ignored",
        },
        client=client,
        limit=25,
        cache_ttl=None,
    )

    assert [row.symbol for row in rows] == ["AAPL"]
    assert rows[0].company_name == "Apple Inc."
    assert rows[0].market_cap == 1000.0
    assert session.calls == [
        (
            "https://financialmodelingprep.com/stable/company-screener",
            {
                "marketCapMoreThan": 1_000_000,
                "sector": "Technology",
                "limit": 25,
                "apikey": "k",
            },
        )
    ]


def test_screen_symbols_uses_filter_limit_when_no_explicit_limit():
    client, session = _client({"company-screener": []})

    rows = screen_symbols(
        {"exchange": "NASDAQ", "limit": 3},
        client=client,
        cache_ttl=None,
    )

    assert rows == []
    assert session.calls[0][1]["limit"] == 3
    assert session.calls[0][1]["exchange"] == "NASDAQ"


def test_screen_symbols_empty_result_is_empty_list():
    client, _ = _client({"company-screener": []})

    assert screen_symbols({}, client=client, cache_ttl=None) == []
