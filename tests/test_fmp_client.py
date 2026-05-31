from __future__ import annotations

import pytest

from screener import cache
from screener.providers.fmp import FmpApiKeyError, FmpClient, FmpRateLimitError
from screener.resilience import RetryConfig


class DummyResponse:
    def __init__(
        self,
        payload: object,
        *,
        status_code: int = 200,
        text: str = "",
        headers: dict[str, str] | None = None,
    ) -> None:
        self.payload = payload
        self.status_code = status_code
        self.text = text
        self.headers = headers or {}

    def json(self) -> object:
        return self.payload


class DummySession:
    def __init__(self, responses: list[DummyResponse]) -> None:
        self.responses = responses
        self.calls: list[tuple[str, dict[str, object], object]] = []

    def get(self, url: str, *, params=None, timeout=None):
        self.calls.append((url, dict(params or {}), timeout))
        return self.responses.pop(0)


def test_fmp_client_requires_api_key(monkeypatch):
    monkeypatch.delenv("FMP_API_KEY", raising=False)

    with pytest.raises(FmpApiKeyError, match="FMP_API_KEY is required"):
        FmpClient(load_env=False)


def test_stable_endpoint_request_construction():
    session = DummySession([DummyResponse({"ok": True})])
    client = FmpClient(api_key="test-key", session=session)

    out = client.get_json(
        "quote/AAPL",
        params={"limit": 1},
        retry=RetryConfig(attempts=1, jitter=0),
    )

    assert out == {"ok": True}
    assert session.calls == [
        (
            "https://financialmodelingprep.com/stable/quote/AAPL",
            {"limit": 1, "apikey": "test-key"},
            30,
        )
    ]


def test_legacy_endpoint_request_construction():
    session = DummySession([DummyResponse([{"symbol": "AAPL"}])])
    client = FmpClient(api_key="test-key", session=session)

    out = client.get_legacy_json(
        "/api/v4/insider-trading",
        params={"symbol": "AAPL", "page": 0},
        retry=RetryConfig(attempts=1, jitter=0),
    )

    assert out == [{"symbol": "AAPL"}]
    assert session.calls == [
        (
            "https://financialmodelingprep.com/api/v4/insider-trading",
            {"symbol": "AAPL", "page": 0, "apikey": "test-key"},
            30,
        )
    ]


def test_429_raises_useful_error():
    session = DummySession(
        [DummyResponse({"error": "slow down"}, status_code=429, text="slow down")]
    )
    client = FmpClient(api_key="test-key", session=session)

    with pytest.raises(FmpRateLimitError, match="rate limit exceeded .*429.*quote/AAPL"):
        client.get_json("quote/AAPL", retry=RetryConfig(attempts=1, jitter=0))


def test_cached_response_path_reuses_disk_cache(tmp_path, monkeypatch):
    monkeypatch.setattr(cache, "CACHE_ROOT", tmp_path)
    session = DummySession([DummyResponse({"price": 10})])
    client = FmpClient(api_key="test-key", session=session)

    first = client.get_json("quote/AAPL", cache_ttl=60)
    second = client.get_json("quote/AAPL", cache_ttl=60)

    assert first == {"price": 10}
    assert second == {"price": 10}
    assert len(session.calls) == 1
