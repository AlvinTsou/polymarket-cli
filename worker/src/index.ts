/**
 * Polymarket Trending Markets — Cloudflare Worker
 *
 * Cron trigger: fetches trending markets from Polymarket Gamma API every 5 min,
 * renders HTML, and caches in KV. HTTP requests serve from KV with CDN caching.
 */

interface Env {
  CACHE_KV: KVNamespace;
}

interface GammaMarket {
  id: string;
  question?: string;
  outcomePrices?: string; // JSON-encoded string array, e.g. '["0.65","0.35"]'
  volumeNum?: string;
  liquidityNum?: string;
  active?: boolean;
  closed?: boolean;
}

interface TrendingMarket {
  question: string;
  price_yes: number;
  price_no: number;
  volume: string;
  liquidity: string;
  status: string;
}

const GAMMA_API = "https://gamma-api.polymarket.com";
const CACHE_HTML_KEY = "trending_html";
const CACHE_JSON_KEY = "trending_json";
const KV_TTL = 600; // 10 min (buffer over 5 min cron)
const CDN_MAX_AGE = 300; // 5 min

// ─── Fetch trending markets from Gamma API ───

async function fetchTrendingMarkets(limit = 20): Promise<GammaMarket[]> {
  const url = `${GAMMA_API}/markets?active=true&closed=false&limit=${limit}&order=volume_num&ascending=false`;
  const resp = await fetch(url, {
    headers: { Accept: "application/json" },
  });
  if (!resp.ok) {
    throw new Error(`Gamma API returned ${resp.status}`);
  }
  return resp.json();
}

// ─── Transform raw market data ───

function parseOutcomePrices(raw?: string): [number, number] {
  if (!raw) return [0, 0];
  try {
    const arr: string[] = JSON.parse(raw);
    return [parseFloat(arr[0] ?? "0"), parseFloat(arr[1] ?? "0")];
  } catch {
    return [0, 0];
  }
}

function formatDecimal(s?: string): string {
  if (!s) return "—";
  const n = parseFloat(s);
  if (isNaN(n)) return "—";
  if (n >= 1_000_000) return `$${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `$${(n / 1_000).toFixed(1)}K`;
  return `$${n.toFixed(2)}`;
}

function toTrending(m: GammaMarket): TrendingMarket {
  const [yes, no] = parseOutcomePrices(m.outcomePrices);
  return {
    question: m.question ?? "",
    price_yes: yes,
    price_no: no,
    volume: formatDecimal(m.volumeNum),
    liquidity: formatDecimal(m.liquidityNum),
    status: m.closed ? "Closed" : m.active ? "Active" : "Inactive",
  };
}

// ─── HTML rendering ───

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function renderCard(m: TrendingMarket): string {
  const yesPct = Math.round(m.price_yes * 100);
  const noPct = Math.max(0, 100 - yesPct);
  return `    <div class="card">
      <div class="question">${escapeHtml(m.question)}</div>
      <div class="bar">
        <div class="yes" style="width:${yesPct}%">${yesPct}% Yes</div>
        <div class="no" style="width:${noPct}%">${noPct}% No</div>
      </div>
      <div class="meta">
        <span>Vol: ${m.volume}</span>
        <span>Liq: ${m.liquidity}</span>
        <span class="status-${m.status.toLowerCase()}">${m.status}</span>
      </div>
    </div>`;
}

function renderPage(markets: TrendingMarket[], now: string): string {
  const cards = markets.map(renderCard).join("\n");
  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <meta http-equiv="refresh" content="${CDN_MAX_AGE}">
  <title>Polymarket Trending</title>
  <style>
    *{margin:0;padding:0;box-sizing:border-box}
    body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;background:#0d1117;color:#c9d1d9;padding:1rem;max-width:900px;margin:0 auto}
    header{display:flex;justify-content:space-between;align-items:center;padding:1rem 0;border-bottom:1px solid #21262d;margin-bottom:1rem}
    h1{font-size:1.4rem;color:#58a6ff}
    .meta-header{font-size:.8rem;color:#8b949e}
    .card{background:#161b22;border:1px solid #21262d;border-radius:8px;padding:1rem;margin-bottom:.75rem}
    .card:hover{border-color:#388bfd44}
    .question{font-size:1rem;font-weight:600;margin-bottom:.6rem;line-height:1.4}
    .bar{display:flex;height:28px;border-radius:6px;overflow:hidden;margin-bottom:.6rem;font-size:.8rem;font-weight:600;line-height:28px;text-align:center}
    .yes{background:#238636;color:#fff;min-width:2rem}
    .no{background:#da3633;color:#fff;min-width:2rem}
    .meta{display:flex;gap:1rem;font-size:.8rem;color:#8b949e}
    .status-active{color:#3fb950}
    .status-closed{color:#f85149}
    .status-inactive{color:#8b949e}
    footer{text-align:center;padding:1.5rem 0;color:#8b949e;font-size:.8rem}
    #countdown{color:#58a6ff}
    @media(max-width:600px){body{padding:.5rem}.question{font-size:.9rem}}
  </style>
</head>
<body>
  <header>
    <h1>Polymarket Trending</h1>
    <div class="meta-header">Updated: ${now}</div>
  </header>
  <main>
${cards}
  </main>
  <footer>
    Next refresh in <span id="countdown">${CDN_MAX_AGE}</span>s
    &middot; Data from <a href="https://polymarket.com" style="color:#58a6ff">Polymarket</a>
  </footer>
  <script>
    (function(){
      let s=${CDN_MAX_AGE};
      const el=document.getElementById('countdown');
      setInterval(()=>{if(s>0)el.textContent=String(--s)},1000);
    })();
  </script>
</body>
</html>`;
}

// ─── Worker handlers ───

async function handleScheduled(env: Env): Promise<void> {
  const rawMarkets = await fetchTrendingMarkets(20);
  const markets = rawMarkets.map(toTrending);
  const now = new Date().toISOString().replace("T", " ").slice(0, 19) + " UTC";

  const html = renderPage(markets, now);
  const json = JSON.stringify({ generated_at: new Date().toISOString(), markets }, null, 2);

  await Promise.all([
    env.CACHE_KV.put(CACHE_HTML_KEY, html, { expirationTtl: KV_TTL }),
    env.CACHE_KV.put(CACHE_JSON_KEY, json, { expirationTtl: KV_TTL }),
  ]);
}

async function handleFetch(request: Request, env: Env): Promise<Response> {
  const url = new URL(request.url);

  if (url.pathname === "/api/trending") {
    const json = await env.CACHE_KV.get(CACHE_JSON_KEY);
    if (!json) {
      return new Response(JSON.stringify({ error: "No data yet. Cron has not run." }), {
        status: 503,
        headers: { "Content-Type": "application/json" },
      });
    }
    return new Response(json, {
      headers: {
        "Content-Type": "application/json",
        "Cache-Control": `public, max-age=${CDN_MAX_AGE}`,
        "Access-Control-Allow-Origin": "*",
      },
    });
  }

  // Default: serve HTML page
  const html = await env.CACHE_KV.get(CACHE_HTML_KEY);
  if (!html) {
    return new Response("<h1>No data yet</h1><p>The cron trigger has not run yet. Check back in a few minutes.</p>", {
      status: 503,
      headers: { "Content-Type": "text/html" },
    });
  }
  return new Response(html, {
    headers: {
      "Content-Type": "text/html;charset=utf-8",
      "Cache-Control": `public, max-age=${CDN_MAX_AGE}`,
    },
  });
}

// ─── Export ───

export default {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    return handleFetch(request, env);
  },

  async scheduled(event: ScheduledEvent, env: Env, ctx: ExecutionContext): Promise<void> {
    ctx.waitUntil(handleScheduled(env));
  },
};
