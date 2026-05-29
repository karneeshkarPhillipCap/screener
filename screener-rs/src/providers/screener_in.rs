use anyhow::Context;
use rayon::prelude::*;
use regex::Regex;
use reqwest::blocking::Client;

#[derive(Debug, Clone, PartialEq)]
pub struct PromoterSnapshot {
    pub symbol: String,
    pub latest_quarter: Option<String>,
    pub promoter_pct_latest: f64,
    pub promoter_pct_prev: f64,
    pub promoter_change: f64,
    pub fii_pct_latest: Option<f64>,
    pub dii_pct_latest: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct ScreenerInClient {
    client: Client,
}

impl ScreenerInClient {
    pub fn new() -> anyhow::Result<Self> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (compatible; screener-rs/0.1)")
            .build()?;
        Ok(Self { client })
    }

    pub fn promoter_snapshot(&self, symbol: &str) -> anyhow::Result<Option<PromoterSnapshot>> {
        let url = format!(
            "https://www.screener.in/company/{}/",
            symbol.trim().to_uppercase()
        );
        let html = self
            .client
            .get(url)
            .send()
            .with_context(|| format!("screener.in request failed for {symbol}"))?
            .error_for_status()
            .with_context(|| format!("screener.in returned non-success for {symbol}"))?
            .text()
            .context("failed to read screener.in HTML")?;
        Ok(parse_promoter_snapshot(symbol, &html))
    }

    pub fn promoter_snapshots(
        &self,
        symbols: &[String],
    ) -> Vec<(String, Option<PromoterSnapshot>)> {
        symbols
            .par_iter()
            .map(|symbol| {
                (
                    symbol.clone(),
                    self.promoter_snapshot(symbol).unwrap_or_default(),
                )
            })
            .collect()
    }
}

pub fn parse_promoter_snapshot(symbol: &str, html: &str) -> Option<PromoterSnapshot> {
    let section_re = Regex::new(
        r#"(?is)<div\s+id=["']quarterly-shp["'][^>]*>(.*?)<div\s+id=["']yearly-shp["']"#,
    )
    .ok()?;
    let section = section_re
        .captures(html)
        .and_then(|caps| caps.get(1).map(|m| m.as_str()))
        .unwrap_or(html);
    let header_re = Regex::new(r#"(?is)<th[^>]*>\s*([^<]+?)\s*</th>"#).ok()?;
    let quarters = header_re
        .captures_iter(section)
        .filter_map(|caps| caps.get(1).map(|m| html_unescape(m.as_str().trim())))
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>();
    let promoters = extract_shareholding_row(section, "Promoters")?;
    if promoters.len() < 2 {
        return None;
    }
    let latest = *promoters.last()?;
    let prev = promoters[promoters.len() - 2];
    Some(PromoterSnapshot {
        symbol: symbol.trim().to_uppercase(),
        latest_quarter: quarters.last().cloned(),
        promoter_pct_latest: latest,
        promoter_pct_prev: prev,
        promoter_change: latest - prev,
        fii_pct_latest: extract_shareholding_row(section, "FIIs").and_then(|v| v.last().copied()),
        dii_pct_latest: extract_shareholding_row(section, "DIIs").and_then(|v| v.last().copied()),
    })
}

fn extract_shareholding_row(section: &str, label: &str) -> Option<Vec<f64>> {
    let row_re = Regex::new(r#"(?is)<tr[^>]*>(.*?)</tr>"#).ok()?;
    let cell_re = Regex::new(r#"(?is)<td[^>]*>\s*([^<]*?)\s*</td>"#).ok()?;
    for caps in row_re.captures_iter(section) {
        let row = caps.get(1)?.as_str();
        if !strip_tags(row)
            .to_lowercase()
            .contains(&label.to_lowercase())
        {
            continue;
        }
        let values = cell_re
            .captures_iter(row)
            .filter_map(|cell| cell.get(1).map(|m| html_unescape(m.as_str())))
            .filter_map(|raw| parse_percent(&raw))
            .collect::<Vec<_>>();
        if !values.is_empty() {
            return Some(values);
        }
    }
    None
}

fn parse_percent(raw: &str) -> Option<f64> {
    raw.replace(['%', ','], "").trim().parse::<f64>().ok()
}

fn strip_tags(raw: &str) -> String {
    let tag_re = Regex::new(r"(?is)<[^>]+>").expect("valid tag regex");
    html_unescape(&tag_re.replace_all(raw, " "))
}

fn html_unescape(raw: &str) -> String {
    raw.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quarterly_promoters() {
        let html = r#"
        <div id="quarterly-shp">
          <table><thead><tr><th></th><th>Dec 2025</th><th>Mar 2026</th></tr></thead>
          <tbody>
            <tr><td class="text"><button>Promoters&nbsp;<span>+</span></button></td><td>50.00%</td><td>50.25%</td></tr>
            <tr><td class="text"><button>FIIs&nbsp;<span>+</span></button></td><td>20.00%</td><td>21.00%</td></tr>
            <tr><td class="text"><button>DIIs&nbsp;<span>+</span></button></td><td>10.00%</td><td>9.00%</td></tr>
          </tbody></table>
        </div><div id="yearly-shp"></div>
        "#;
        let parsed = parse_promoter_snapshot("abc", html).unwrap();
        assert_eq!(parsed.symbol, "ABC");
        assert_eq!(parsed.latest_quarter.as_deref(), Some("Mar 2026"));
        assert_eq!(parsed.promoter_pct_latest, 50.25);
        assert_eq!(parsed.promoter_pct_prev, 50.0);
        assert_eq!(parsed.promoter_change, 0.25);
        assert_eq!(parsed.fii_pct_latest, Some(21.0));
        assert_eq!(parsed.dii_pct_latest, Some(9.0));
    }
}
