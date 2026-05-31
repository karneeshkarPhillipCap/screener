from __future__ import annotations

from datetime import date
from pathlib import Path

import pytest

import screener.universes as universes
from screener.providers.fmp_screener import ScreenerRow
from screener.universes import Universe


def test_build_fmp_universe_normalizes_us_symbols_and_dedupes(monkeypatch, tmp_path):
    monkeypatch.setattr(universes, "CACHE_DIR", tmp_path)

    def fake_screen_symbols(filters, *, client=None, limit=None, refresh=False):
        return [
            ScreenerRow(symbol=" brk.b "),
            ScreenerRow(symbol="BRK-B"),
            ScreenerRow(symbol=""),
        ]

    monkeypatch.setattr(universes, "screen_symbols", fake_screen_symbols)

    universe = universes.build_fmp_universe(
        filters={"exchange": "NYSE"},
        as_of=date(2025, 1, 2),
        use_cache=False,
    )

    assert universe.symbols == ("BRK-B",)
    assert universe.source == "fmp:company-screener"
    assert universe.cached_path.exists()


def test_build_fmp_universe_intersects_static_base(monkeypatch, tmp_path):
    monkeypatch.setattr(universes, "CACHE_DIR", tmp_path)

    def fake_screen_symbols(filters, *, client=None, limit=None, refresh=False):
        return [ScreenerRow(symbol="AAPL"), ScreenerRow(symbol="MSFT")]

    def fake_load_current_universe(name, *, as_of, use_cache=True):
        return Universe(
            name=name,
            symbols=("AAPL", "BRK-B"),
            source="test",
            cached_path=Path("/tmp/base-universe.txt"),
        )

    monkeypatch.setattr(universes, "screen_symbols", fake_screen_symbols)
    monkeypatch.setattr(universes, "load_current_universe", fake_load_current_universe)

    universe = universes.build_fmp_universe(
        filters={"sector": "Technology"},
        base="sp500",
        as_of=date(2025, 1, 2),
        use_cache=False,
    )

    assert universe.symbols == ("AAPL",)
    assert universe.source == "fmp:company-screener; base=sp500"


def test_build_fmp_universe_empty_result_raises(monkeypatch, tmp_path):
    monkeypatch.setattr(universes, "CACHE_DIR", tmp_path)
    monkeypatch.setattr(
        universes,
        "screen_symbols",
        lambda filters, *, client=None, limit=None, refresh=False: [],
    )

    with pytest.raises(RuntimeError, match="no symbols"):
        universes.build_fmp_universe(
            filters={"sector": "Missing"},
            as_of=date(2025, 1, 2),
            use_cache=False,
        )


def test_build_fmp_universe_cache_round_trip(monkeypatch, tmp_path):
    monkeypatch.setattr(universes, "CACHE_DIR", tmp_path)
    calls = 0

    def fake_screen_symbols(filters, *, client=None, limit=None, refresh=False):
        nonlocal calls
        calls += 1
        return [ScreenerRow(symbol="AAPL")]

    monkeypatch.setattr(universes, "screen_symbols", fake_screen_symbols)

    first = universes.build_fmp_universe(
        filters={"exchange": "NASDAQ"},
        as_of=date(2025, 1, 2),
    )
    second = universes.build_fmp_universe(
        filters={"exchange": "NASDAQ"},
        as_of=date(2025, 1, 2),
    )

    assert calls == 1
    assert first.symbols == ("AAPL",)
    assert second.symbols == ("AAPL",)
    assert second.cached_path == first.cached_path
    assert first.cached_path.read_text().splitlines()[:3] == [
        f"# universe={first.name}",
        "# as_of=2025-01-02",
        "# source=fmp:company-screener",
    ]


def test_build_fmp_universe_keeps_india_symbols_as_symbols(monkeypatch, tmp_path):
    monkeypatch.setattr(universes, "CACHE_DIR", tmp_path)
    monkeypatch.setattr(
        universes,
        "screen_symbols",
        lambda filters, *, client=None, limit=None, refresh=False: [
            ScreenerRow(symbol=" reliance.ns ")
        ],
    )

    universe = universes.build_fmp_universe(
        filters={"country": "IN"},
        market="india",
        as_of=date(2025, 1, 2),
        use_cache=False,
    )

    assert universe.symbols == ("RELIANCE.NS",)
