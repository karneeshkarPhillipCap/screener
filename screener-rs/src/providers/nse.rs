use anyhow::Context;
use chrono::{Datelike, Duration, NaiveDate};
use rayon::prelude::*;
use reqwest::blocking::Client;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;
use zip::ZipArchive;

#[derive(Debug, Clone, PartialEq)]
pub struct CashBhavcopyRow {
    pub symbol: String,
    pub date: NaiveDate,
    pub prev_close: f64,
    pub close_price: f64,
    pub avg_price: f64,
    pub ttl_trd_qnty: f64,
    pub deliv_qty: f64,
    pub deliv_per: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeliveryRow {
    pub symbol: String,
    pub date: NaiveDate,
    pub ttl_trd_qnty: f64,
    pub deliv_qty: f64,
    pub deliv_per: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FoBhavcopyRow {
    pub symbol: String,
    pub expiry: NaiveDate,
    pub oi: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NearMonthOi {
    pub symbol: String,
    pub current_oi: Option<f64>,
    pub next_oi: Option<f64>,
    pub cumulative_oi: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct NseClient {
    client: Client,
}

impl NseClient {
    pub fn new() -> anyhow::Result<Self> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36")
            .build()?;
        Ok(Self { client })
    }

    pub fn latest_trading_day(&self, day: NaiveDate, lookback: i64) -> anyhow::Result<NaiveDate> {
        for delta in 0..=lookback {
            let candidate = day - Duration::days(delta);
            if let Ok(rows) = self.fetch_cash_bhavcopy(candidate)
                && let Some(actual) = rows.first().map(|row| row.date)
            {
                return Ok(actual);
            }
        }
        anyhow::bail!("no NSE cash bhavcopy found within {lookback} days of {day}")
    }

    pub fn fetch_cash_bhavcopy(&self, day: NaiveDate) -> anyhow::Result<Vec<CashBhavcopyRow>> {
        let url = format!(
            "https://nsearchives.nseindia.com/products/content/sec_bhavdata_full_{:02}{:02}{}.csv",
            day.day(),
            day.month(),
            day.year()
        );
        let bytes = self
            .client
            .get(url)
            .header("accept", "text/csv,*/*;q=0.8")
            .send()
            .context("NSE cash bhavcopy request failed")?
            .error_for_status()
            .context("NSE cash bhavcopy returned non-success")?
            .bytes()
            .context("failed reading NSE cash bhavcopy bytes")?;
        parse_cash_bhavcopy(&bytes)
    }

    pub fn fetch_fo_bhavcopy(&self, day: NaiveDate) -> anyhow::Result<Vec<FoBhavcopyRow>> {
        let url = format!(
            "https://nsearchives.nseindia.com/content/fo/BhavCopy_NSE_FO_0_0_0_{}_F_0000.csv.zip",
            day.format("%Y%m%d")
        );
        let bytes = self
            .client
            .get(url)
            .header("accept", "application/zip,*/*;q=0.8")
            .send()
            .context("NSE FO bhavcopy request failed")?
            .error_for_status()
            .context("NSE FO bhavcopy returned non-success")?
            .bytes()
            .context("failed reading NSE FO bhavcopy bytes")?;
        parse_fo_zip(&bytes)
    }

    pub fn fno_ban_list(&self) -> anyhow::Result<BTreeSet<String>> {
        let text = self
            .client
            .get("https://nsearchives.nseindia.com/content/fo/fo_secban.csv")
            .header("accept", "text/csv,*/*;q=0.8")
            .send()
            .context("NSE F&O ban request failed")?
            .error_for_status()
            .context("NSE F&O ban returned non-success")?
            .text()
            .context("failed reading NSE F&O ban text")?;
        Ok(parse_fno_ban_csv(&text))
    }

    pub fn load_delivery_panel(
        &self,
        symbols: &[String],
        as_of: NaiveDate,
        history_days: i64,
    ) -> Vec<DeliveryRow> {
        let sym_set = symbols
            .iter()
            .map(|s| s.trim().to_uppercase())
            .collect::<BTreeSet<_>>();
        let earliest = as_of - Duration::days(history_days);
        let mut day = as_of;
        let mut days = Vec::new();
        while day >= earliest {
            if day.weekday().number_from_monday() <= 5 {
                days.push(day);
            }
            day -= Duration::days(1);
        }
        let mut rows = days
            .par_iter()
            .filter_map(|day| self.fetch_cash_bhavcopy(*day).ok())
            .flat_map_iter(|cash| {
                cash.into_iter().filter_map(|row| {
                    sym_set.contains(&row.symbol).then_some(DeliveryRow {
                        symbol: row.symbol,
                        date: row.date,
                        ttl_trd_qnty: row.ttl_trd_qnty,
                        deliv_qty: row.deliv_qty,
                        deliv_per: row.deliv_per,
                    })
                })
            })
            .collect::<Vec<_>>();
        rows.sort_by(|a, b| (&a.symbol, a.date).cmp(&(&b.symbol, b.date)));
        rows
    }
}

pub fn near_month_oi(rows: &[FoBhavcopyRow]) -> Vec<NearMonthOi> {
    let mut grouped: BTreeMap<String, Vec<&FoBhavcopyRow>> = BTreeMap::new();
    for row in rows {
        grouped.entry(row.symbol.clone()).or_default().push(row);
    }
    grouped
        .into_iter()
        .map(|(symbol, mut rows)| {
            rows.sort_by_key(|row| row.expiry);
            let current = rows.first().map(|row| row.oi);
            let next = rows.get(1).map(|row| row.oi);
            NearMonthOi {
                symbol,
                current_oi: current,
                next_oi: next,
                cumulative_oi: current.map(|cur| cur + next.unwrap_or(0.0)),
            }
        })
        .collect()
}

pub fn parse_cash_bhavcopy(bytes: &[u8]) -> anyhow::Result<Vec<CashBhavcopyRow>> {
    let mut rdr = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(bytes);
    let headers = rdr
        .headers()?
        .iter()
        .map(|h| h.trim().to_string())
        .collect::<Vec<_>>();
    let mut out = Vec::new();
    for record in rdr.records() {
        let record = record?;
        let series = value(&headers, &record, "SERIES").unwrap_or_default();
        if series != "EQ" {
            continue;
        }
        let date_raw = value(&headers, &record, "DATE1").context("missing DATE1")?;
        out.push(CashBhavcopyRow {
            symbol: value(&headers, &record, "SYMBOL")
                .context("missing SYMBOL")?
                .to_string(),
            date: parse_nse_date(&date_raw)?,
            prev_close: parse_num(&value(&headers, &record, "PREV_CLOSE").unwrap_or_default()),
            close_price: parse_num(&value(&headers, &record, "CLOSE_PRICE").unwrap_or_default()),
            avg_price: parse_num(&value(&headers, &record, "AVG_PRICE").unwrap_or_default()),
            ttl_trd_qnty: parse_num(&value(&headers, &record, "TTL_TRD_QNTY").unwrap_or_default()),
            deliv_qty: parse_num(&value(&headers, &record, "DELIV_QTY").unwrap_or_default()),
            deliv_per: parse_num(&value(&headers, &record, "DELIV_PER").unwrap_or_default()),
        });
    }
    Ok(out)
}

pub fn parse_fo_zip(bytes: &[u8]) -> anyhow::Result<Vec<FoBhavcopyRow>> {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).context("invalid NSE FO zip")?;
    let mut file = zip.by_index(0).context("empty NSE FO zip")?;
    let mut rdr = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(&mut file);
    let headers = rdr
        .headers()?
        .iter()
        .map(|h| h.trim().to_string())
        .collect::<Vec<_>>();
    let mut out = Vec::new();
    for record in rdr.records() {
        let record = record?;
        if value(&headers, &record, "FinInstrmTp").as_deref() != Some("STF") {
            continue;
        }
        let expiry_raw = value(&headers, &record, "XpryDt").context("missing XpryDt")?;
        out.push(FoBhavcopyRow {
            symbol: value(&headers, &record, "TckrSymb")
                .context("missing TckrSymb")?
                .to_string(),
            expiry: parse_iso_date(&expiry_raw)?,
            oi: parse_num(&value(&headers, &record, "OpnIntrst").unwrap_or_default()),
        });
    }
    Ok(out)
}

pub fn parse_fno_ban_csv(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.to_lowercase().starts_with("securities in ban") {
            continue;
        }
        let parts = line.split(',').map(str::trim).collect::<Vec<_>>();
        if let Some(symbol) = parts.get(1).filter(|s| !s.is_empty()) {
            out.insert(symbol.to_uppercase());
        } else if parts.len() == 1 && parts[0].chars().all(|c| c.is_ascii_alphabetic()) {
            out.insert(parts[0].to_uppercase());
        }
    }
    out
}

fn value(headers: &[String], record: &csv::StringRecord, key: &str) -> Option<String> {
    headers
        .iter()
        .position(|h| h == key)
        .and_then(|idx| record.get(idx))
        .map(str::trim)
        .map(str::to_string)
}

fn parse_num(raw: &str) -> f64 {
    raw.replace(',', "").parse::<f64>().unwrap_or(f64::NAN)
}

fn parse_nse_date(raw: &str) -> anyhow::Result<NaiveDate> {
    NaiveDate::parse_from_str(raw.trim(), "%d-%b-%Y")
        .or_else(|_| NaiveDate::parse_from_str(raw.trim(), "%d-%b-%y"))
        .with_context(|| format!("invalid NSE date {raw:?}"))
}

fn parse_iso_date(raw: &str) -> anyhow::Result<NaiveDate> {
    NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(raw.trim(), "%d-%b-%Y"))
        .with_context(|| format!("invalid FO expiry date {raw:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cash_bhavcopy_rows() {
        let csv = b"SYMBOL,SERIES,DATE1,PREV_CLOSE,CLOSE_PRICE,AVG_PRICE,TTL_TRD_QNTY,DELIV_QTY,DELIV_PER\nABC,EQ,01-Jan-2026,10,11,10.5,1000,700,70\nXYZ,BE,01-Jan-2026,10,11,10.5,1000,700,70\n";
        let rows = parse_cash_bhavcopy(csv).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].symbol, "ABC");
        assert_eq!(rows[0].deliv_per, 70.0);
    }

    #[test]
    fn collapses_near_month_oi() {
        let rows = vec![
            FoBhavcopyRow {
                symbol: "ABC".to_string(),
                expiry: NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
                oi: 30.0,
            },
            FoBhavcopyRow {
                symbol: "ABC".to_string(),
                expiry: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                oi: 20.0,
            },
        ];
        let out = near_month_oi(&rows);
        assert_eq!(out[0].current_oi, Some(20.0));
        assert_eq!(out[0].cumulative_oi, Some(50.0));
    }

    #[test]
    fn parses_ban_list() {
        let out = parse_fno_ban_csv("Securities in Ban For Trade Date 01-JUN-2026:\n1,SAIL\n");
        assert!(out.contains("SAIL"));
    }
}
