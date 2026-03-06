use chrono::Utc;
use polymarket_client_sdk::gamma::types::response::Market;
use rust_decimal::prelude::ToPrimitive;

use super::format_decimal;

struct TrendingMarket {
    question: String,
    price_yes: f64,
    price_no: f64,
    volume: String,
    liquidity: String,
    status: &'static str,
}

fn market_status(m: &Market) -> &'static str {
    if m.closed == Some(true) {
        "Closed"
    } else if m.active == Some(true) {
        "Active"
    } else {
        "Inactive"
    }
}

fn to_trending(m: &Market) -> TrendingMarket {
    let prices = m.outcome_prices.as_ref();
    let yes = prices
        .and_then(|p| p.first())
        .and_then(|d| d.to_f64())
        .unwrap_or(0.0);
    let no = prices
        .and_then(|p| p.get(1))
        .and_then(|d| d.to_f64())
        .unwrap_or(0.0);

    TrendingMarket {
        question: m.question.clone().unwrap_or_default(),
        price_yes: yes,
        price_no: no,
        volume: m
            .volume_num
            .map(format_decimal)
            .unwrap_or_else(|| "—".into()),
        liquidity: m
            .liquidity_num
            .map(format_decimal)
            .unwrap_or_else(|| "—".into()),
        status: market_status(m),
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn render_market_card(m: &TrendingMarket) -> String {
    let yes_pct = (m.price_yes * 100.0).round() as u32;
    let no_pct = 100u32.saturating_sub(yes_pct);
    let question = escape_html(&m.question);

    format!(
        r#"    <div class="card">
      <div class="question">{question}</div>
      <div class="bar">
        <div class="yes" style="width:{yes_pct}%">{yes_pct}% Yes</div>
        <div class="no" style="width:{no_pct}%">{no_pct}% No</div>
      </div>
      <div class="meta">
        <span>Vol: {vol}</span>
        <span>Liq: {liq}</span>
        <span class="status-{status_lower}">{status}</span>
      </div>
    </div>"#,
        vol = m.volume,
        liq = m.liquidity,
        status = m.status,
        status_lower = m.status.to_lowercase(),
    )
}

pub fn print_trending_html(markets: &[Market], title: &str, refresh_secs: u32) -> String {
    let now = Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
    let title_escaped = escape_html(title);

    let cards: String = markets
        .iter()
        .map(to_trending)
        .map(|m| render_market_card(&m))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <meta http-equiv="refresh" content="{refresh_secs}">
  <title>{title_escaped}</title>
  <style>
    *{{margin:0;padding:0;box-sizing:border-box}}
    body{{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;background:#0d1117;color:#c9d1d9;padding:1rem;max-width:900px;margin:0 auto}}
    header{{display:flex;justify-content:space-between;align-items:center;padding:1rem 0;border-bottom:1px solid #21262d;margin-bottom:1rem}}
    h1{{font-size:1.4rem;color:#58a6ff}}
    .meta-header{{font-size:.8rem;color:#8b949e}}
    .card{{background:#161b22;border:1px solid #21262d;border-radius:8px;padding:1rem;margin-bottom:.75rem}}
    .card:hover{{border-color:#388bfd44}}
    .question{{font-size:1rem;font-weight:600;margin-bottom:.6rem;line-height:1.4}}
    .bar{{display:flex;height:28px;border-radius:6px;overflow:hidden;margin-bottom:.6rem;font-size:.8rem;font-weight:600;line-height:28px;text-align:center}}
    .yes{{background:#238636;color:#fff;min-width:2rem}}
    .no{{background:#da3633;color:#fff;min-width:2rem}}
    .meta{{display:flex;gap:1rem;font-size:.8rem;color:#8b949e}}
    .status-active{{color:#3fb950}}
    .status-closed{{color:#f85149}}
    .status-inactive{{color:#8b949e}}
    footer{{text-align:center;padding:1.5rem 0;color:#8b949e;font-size:.8rem}}
    #countdown{{color:#58a6ff}}
    @media(max-width:600px){{body{{padding:.5rem}}.question{{font-size:.9rem}}}}
  </style>
</head>
<body>
  <header>
    <h1>{title_escaped}</h1>
    <div class="meta-header">Updated: {now}</div>
  </header>
  <main>
{cards}
  </main>
  <footer>
    Next refresh in <span id="countdown">{refresh_secs}</span>s
    &middot; Data from <a href="https://polymarket.com" style="color:#58a6ff">Polymarket</a>
  </footer>
  <script>
    (function(){{
      let s={refresh_secs};
      const el=document.getElementById('countdown');
      setInterval(()=>{{if(s>0)el.textContent=--s}},1000);
    }})();
  </script>
</body>
</html>"#,
    )
}

#[derive(serde::Serialize)]
struct TrendingJsonMarket {
    question: String,
    price_yes: f64,
    price_no: f64,
    volume: String,
    liquidity: String,
    status: String,
}

#[derive(serde::Serialize)]
struct TrendingJsonOutput {
    generated_at: String,
    markets: Vec<TrendingJsonMarket>,
}

pub fn print_trending_json(markets: &[Market]) -> anyhow::Result<String> {
    let output = TrendingJsonOutput {
        generated_at: Utc::now().to_rfc3339(),
        markets: markets
            .iter()
            .map(to_trending)
            .map(|m| TrendingJsonMarket {
                question: m.question,
                price_yes: m.price_yes,
                price_no: m.price_no,
                volume: m.volume,
                liquidity: m.liquidity,
                status: m.status.to_string(),
            })
            .collect(),
    };
    Ok(serde_json::to_string_pretty(&output)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_market(val: serde_json::Value) -> Market {
        serde_json::from_value(val).unwrap()
    }

    #[test]
    fn html_contains_title_and_refresh() {
        let markets = vec![make_market(json!({
            "id": "1",
            "question": "Will it rain?",
            "outcomePrices": "[\"0.70\",\"0.30\"]",
            "active": true
        }))];
        let html = print_trending_html(&markets, "My Title", 300);
        assert!(html.contains("My Title"));
        assert!(html.contains("content=\"300\""));
        assert!(html.contains("70% Yes"));
        assert!(html.contains("30% No"));
    }

    #[test]
    fn html_escapes_special_chars() {
        let markets = vec![make_market(json!({
            "id": "1",
            "question": "Is 2 < 3 & 4 > 1?"
        }))];
        let html = print_trending_html(&markets, "Test <b>Title</b>", 60);
        assert!(html.contains("Test &lt;b&gt;Title&lt;/b&gt;"));
        assert!(html.contains("Is 2 &lt; 3 &amp; 4 &gt; 1?"));
    }

    #[test]
    fn html_empty_markets_produces_valid_page() {
        let html = print_trending_html(&[], "Empty", 300);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("</html>"));
    }

    #[test]
    fn json_output_structure() {
        let markets = vec![make_market(json!({
            "id": "1",
            "question": "Test?",
            "outcomePrices": "[\"0.65\",\"0.35\"]",
            "volumeNum": "1500000",
            "liquidityNum": "2500",
            "active": true
        }))];
        let json_str = print_trending_json(&markets).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(parsed["generated_at"].is_string());
        assert_eq!(parsed["markets"][0]["price_yes"], 0.65);
        assert_eq!(parsed["markets"][0]["price_no"], 0.35);
        assert_eq!(parsed["markets"][0]["volume"], "$1.5M");
        assert_eq!(parsed["markets"][0]["status"], "Active");
    }

    #[test]
    fn json_empty_markets() {
        let json_str = print_trending_json(&[]).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["markets"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn to_trending_missing_prices() {
        let m = to_trending(&make_market(json!({"id": "1"})));
        assert_eq!(m.price_yes, 0.0);
        assert_eq!(m.price_no, 0.0);
        assert_eq!(m.volume, "—");
    }

    #[test]
    fn market_card_renders_percentages() {
        let m = TrendingMarket {
            question: "Test?".into(),
            price_yes: 0.82,
            price_no: 0.18,
            volume: "$1.0M".into(),
            liquidity: "$500.0K".into(),
            status: "Active",
        };
        let card = render_market_card(&m);
        assert!(card.contains("82% Yes"));
        assert!(card.contains("18% No"));
        assert!(card.contains("width:82%"));
    }
}
