use super::{Candle, Direction, FuturesData, MomentumSignal, OrderBook, SignalComponents, Trade, CryptoAsset};

/// Weights for basic signal (no futures data).
const W_MOM_1M: f64 = 0.30;
const W_MOM_5M: f64 = 0.25;
const W_OB: f64 = 0.25;
const W_FLOW: f64 = 0.20;

/// Weights for enhanced signal (with futures data).
const WE_MOM_1M: f64 = 0.15;
const WE_MOM_5M: f64 = 0.10;
const WE_OB: f64 = 0.20;
const WE_FLOW: f64 = 0.20;
const WE_FUNDING: f64 = 0.15;
const WE_OI: f64 = 0.10;
const WE_LIQ: f64 = 0.10;

/// Typical BTC 8h funding rate stddev (~0.01% = 0.0001).
const FUNDING_STDDEV: f64 = 0.0001;

/// Recent liquidations window (5 minutes in ms).
const LIQ_WINDOW_MS: i64 = 5 * 60 * 1000;

/// Skip if volatility exceeds this threshold (stddev of 1m returns).
const VOL_THRESHOLD: f64 = 0.003; // 0.3%

/// Skip if |score| below this threshold.
const SIGNAL_THRESHOLD: f64 = 0.10;

/// High confidence threshold.
const HIGH_CONFIDENCE: f64 = 0.30;

/// Compute momentum signal from exchange data (basic, no futures).
///
/// `candles`: recent 1m candles (at least 6 needed for 5m lookback).
/// `orderbook`: current order book snapshot.
/// `trades`: recent trades (last ~500).
pub fn compute_signal(
    asset: CryptoAsset,
    candles: &[Candle],
    orderbook: &OrderBook,
    trades: &[Trade],
) -> MomentumSignal {
    compute_signal_enhanced(asset, candles, orderbook, trades, None)
}

/// Compute momentum signal with optional futures data (enhanced 7-component model).
pub fn compute_signal_enhanced(
    asset: CryptoAsset,
    candles: &[Candle],
    orderbook: &OrderBook,
    trades: &[Trade],
    futures: Option<&FuturesData>,
) -> MomentumSignal {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let current_price = candles.last().map(|c| c.close).unwrap_or(0.0);

    let price_mom_1m = price_momentum(candles, 1);
    let price_mom_5m = price_momentum(candles, 5);
    let ob_imbalance = orderbook_imbalance(orderbook);
    let trade_flow = trade_flow_imbalance(trades);
    let volatility = return_volatility(candles, 15);

    let (funding_signal, oi_delta_signal, liquidation_signal, raw_score) = match futures {
        Some(fd) => {
            let fs = compute_funding_signal(fd.funding_rate);
            let oi_s = compute_oi_signal(price_mom_5m, fd.open_interest_usd, current_price);
            let liq_s = compute_liquidation_signal(&fd.liquidations, now_ms);

            let score = WE_MOM_1M * price_mom_1m
                + WE_MOM_5M * price_mom_5m
                + WE_OB * ob_imbalance
                + WE_FLOW * trade_flow
                + WE_FUNDING * fs
                + WE_OI * oi_s
                + WE_LIQ * liq_s;

            (fs, oi_s, liq_s, score)
        }
        None => {
            let score = W_MOM_1M * price_mom_1m
                + W_MOM_5M * price_mom_5m
                + W_OB * ob_imbalance
                + W_FLOW * trade_flow;
            (0.0, 0.0, 0.0, score)
        }
    };

    let (direction, confidence) = if volatility > VOL_THRESHOLD {
        (Direction::Skip, 0.0)
    } else if raw_score.abs() < SIGNAL_THRESHOLD {
        (Direction::Skip, 0.0)
    } else {
        let dir = if raw_score > 0.0 {
            Direction::Up
        } else {
            Direction::Down
        };
        let conf = (raw_score.abs() / HIGH_CONFIDENCE).min(1.0);
        (dir, conf)
    };

    MomentumSignal {
        asset,
        direction,
        confidence,
        components: SignalComponents {
            price_mom_1m,
            price_mom_5m,
            ob_imbalance,
            trade_flow,
            volatility,
            funding_signal,
            oi_delta_signal,
            liquidation_signal,
            raw_score,
        },
        price: current_price,
        timestamp: now_ms,
    }
}

/// Contrarian funding signal: extreme positive funding → shorts squeezed out → expect DOWN.
/// Extreme negative funding → longs squeezed → expect UP.
/// Normalized to [-1, +1] using ~2 stddev of typical BTC funding.
fn compute_funding_signal(funding_rate: f64) -> f64 {
    // Negate: high positive funding = crowded longs = contrarian bearish
    let normalized = -funding_rate / (2.0 * FUNDING_STDDEV);
    normalized.clamp(-1.0, 1.0)
}

/// OI delta signal: OI increasing with price = momentum continuation.
/// We use a proxy: if price is going up (positive mom) and OI is large, momentum is backed by leverage.
/// Simple approach: sign(price_mom) scaled by OI magnitude relative to typical.
fn compute_oi_signal(price_mom_5m: f64, oi_usd: f64, current_price: f64) -> f64 {
    if oi_usd == 0.0 || current_price == 0.0 {
        return 0.0;
    }
    // OI ratio: contracts / price gives a rough leverage indicator
    // We just use the direction of price momentum as the signal,
    // scaled slightly by whether OI is non-trivial (it always is for BTC).
    // This is a simplified version — Phase 2 will track OI delta over time.
    if price_mom_5m.abs() < 0.0001 {
        0.0
    } else {
        price_mom_5m.signum() * (price_mom_5m.abs() * 100.0).min(1.0)
    }
}

/// Liquidation imbalance: net short liquidations (BUY side) = upward pressure.
/// Net long liquidations (SELL side) = downward pressure.
/// Only considers liquidations within the last 5 minutes.
fn compute_liquidation_signal(liquidations: &[super::Liquidation], now_ms: i64) -> f64 {
    let cutoff = now_ms - LIQ_WINDOW_MS;
    let mut buy_vol = 0.0; // short liquidations (forced buy)
    let mut sell_vol = 0.0; // long liquidations (forced sell)

    for l in liquidations {
        if l.time < cutoff {
            continue;
        }
        let notional = l.price * l.qty;
        if l.side == "BUY" {
            buy_vol += notional;
        } else {
            sell_vol += notional;
        }
    }

    let total = buy_vol + sell_vol;
    if total == 0.0 {
        return 0.0;
    }
    // Positive = more short liquidations = upward pressure
    (buy_vol - sell_vol) / total
}

/// Price return over last `n` candles. Positive = price went up.
/// Normalized by current price to get percentage return.
fn price_momentum(candles: &[Candle], n: usize) -> f64 {
    if candles.len() < n + 1 {
        return 0.0;
    }
    let current = candles[candles.len() - 1].close;
    let past = candles[candles.len() - 1 - n].close;
    if past == 0.0 {
        return 0.0;
    }
    (current - past) / past
}

/// Order book imbalance: (bid_vol - ask_vol) / (bid_vol + ask_vol).
/// Positive = more buy pressure.
fn orderbook_imbalance(ob: &OrderBook) -> f64 {
    let bid_vol: f64 = ob.bids.iter().map(|l| l.qty).sum();
    let ask_vol: f64 = ob.asks.iter().map(|l| l.qty).sum();
    let total = bid_vol + ask_vol;
    if total == 0.0 {
        return 0.0;
    }
    (bid_vol - ask_vol) / total
}

/// Trade flow imbalance: (buy_vol - sell_vol) / total_vol.
/// In Binance, `is_buyer_maker = true` means taker sold (aggressive sell).
fn trade_flow_imbalance(trades: &[Trade]) -> f64 {
    let mut buy_vol = 0.0;
    let mut sell_vol = 0.0;
    for t in trades {
        let notional = t.price * t.qty;
        if t.is_buyer_maker {
            sell_vol += notional;
        } else {
            buy_vol += notional;
        }
    }
    let total = buy_vol + sell_vol;
    if total == 0.0 {
        return 0.0;
    }
    (buy_vol - sell_vol) / total
}

/// Standard deviation of 1-minute returns over the last `n` candles.
fn return_volatility(candles: &[Candle], n: usize) -> f64 {
    if candles.len() < 2 {
        return 0.0;
    }
    let start = if candles.len() > n { candles.len() - n } else { 0 };
    let returns: Vec<f64> = candles[start..]
        .windows(2)
        .filter_map(|w| {
            if w[0].close == 0.0 {
                None
            } else {
                Some((w[1].close - w[0].close) / w[0].close)
            }
        })
        .collect();

    if returns.is_empty() {
        return 0.0;
    }
    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
    variance.sqrt()
}

/// Compute a candle-based proxy for orderbook imbalance.
/// Uses recent candle buying pressure: (close - low) / (high - low) averaged.
fn candle_ob_proxy(candles: &[Candle], n: usize) -> f64 {
    if candles.len() < 2 {
        return 0.0;
    }
    let start = if candles.len() > n { candles.len() - n } else { 0 };
    let values: Vec<f64> = candles[start..]
        .iter()
        .filter_map(|c| {
            let range = c.high - c.low;
            if range == 0.0 {
                None
            } else {
                // (close - low) / range: 1.0 = closed at high (bullish), 0.0 = closed at low
                Some((c.close - c.low) / range)
            }
        })
        .collect();
    if values.is_empty() {
        return 0.0;
    }
    let avg = values.iter().sum::<f64>() / values.len() as f64;
    // Map 0-1 to -1..+1
    (avg - 0.5) * 2.0
}

/// Compute a candle-based proxy for trade flow imbalance.
/// Uses volume-weighted direction: positive volume if close > open.
fn candle_flow_proxy(candles: &[Candle], n: usize) -> f64 {
    if candles.len() < 2 {
        return 0.0;
    }
    let start = if candles.len() > n { candles.len() - n } else { 0 };
    let mut buy_vol = 0.0;
    let mut sell_vol = 0.0;
    for c in &candles[start..] {
        if c.close >= c.open {
            buy_vol += c.volume;
        } else {
            sell_vol += c.volume;
        }
    }
    let total = buy_vol + sell_vol;
    if total == 0.0 {
        return 0.0;
    }
    (buy_vol - sell_vol) / total
}

/// Compute signal using only candle data (for backtest without live OB/trades).
fn compute_signal_from_candles(asset: CryptoAsset, candles: &[Candle]) -> MomentumSignal {
    let now_ms = candles.last().map(|c| c.close_time).unwrap_or(0);
    let current_price = candles.last().map(|c| c.close).unwrap_or(0.0);

    let price_mom_1m = price_momentum(candles, 1);
    let price_mom_5m = price_momentum(candles, 5);
    let ob_imbalance = candle_ob_proxy(candles, 5);
    let trade_flow = candle_flow_proxy(candles, 5);
    let volatility = return_volatility(candles, 15);

    let raw_score =
        W_MOM_1M * price_mom_1m + W_MOM_5M * price_mom_5m + W_OB * ob_imbalance + W_FLOW * trade_flow;

    let (direction, confidence) = if volatility > VOL_THRESHOLD {
        (Direction::Skip, 0.0)
    } else if raw_score.abs() < SIGNAL_THRESHOLD {
        (Direction::Skip, 0.0)
    } else {
        let dir = if raw_score > 0.0 {
            Direction::Up
        } else {
            Direction::Down
        };
        let conf = (raw_score.abs() / HIGH_CONFIDENCE).min(1.0);
        (dir, conf)
    };

    MomentumSignal {
        asset,
        direction,
        confidence,
        components: SignalComponents {
            price_mom_1m,
            price_mom_5m,
            ob_imbalance,
            trade_flow,
            volatility,
            funding_signal: 0.0,
            oi_delta_signal: 0.0,
            liquidation_signal: 0.0,
            raw_score,
        },
        price: current_price,
        timestamp: now_ms,
    }
}

/// Backtest: given historical 1m candles, simulate signals for each 5m window
/// and compare predicted direction to actual BTC/ETH price movement.
pub fn backtest_signals(asset: CryptoAsset, candles: &[Candle]) -> BacktestResult {
    let mut total_windows = 0u32;
    let mut signals_generated = 0u32;
    let mut correct = 0u32;
    let mut wrong = 0u32;
    let mut details = Vec::new();

    let min_history = 15;
    if candles.len() < min_history + 5 {
        return BacktestResult {
            total_windows: 0,
            signals_generated: 0,
            correct: 0,
            wrong: 0,
            win_rate: 0.0,
            skip_rate: 0.0,
            details,
        };
    }

    let mut i = min_history;
    while i + 5 <= candles.len() {
        total_windows += 1;

        let history = &candles[..i];
        let signal = compute_signal_from_candles(asset, history);

        let window_open = candles[i].open;
        let window_close = candles[i + 4].close;
        let actual_direction = if window_close > window_open {
            Direction::Up
        } else if window_close < window_open {
            Direction::Down
        } else {
            Direction::Skip
        };

        if signal.direction != Direction::Skip {
            signals_generated += 1;
            let is_correct = signal.direction == actual_direction;
            if is_correct {
                correct += 1;
            } else {
                wrong += 1;
            }
            details.push(BacktestEntry {
                time: candles[i].open_time,
                predicted: signal.direction,
                actual: actual_direction,
                confidence: signal.confidence,
                price_at_signal: signal.price,
                window_open,
                window_close,
                correct: is_correct,
            });
        }

        i += 5;
    }

    let win_rate = if signals_generated > 0 {
        correct as f64 / signals_generated as f64
    } else {
        0.0
    };
    let skip_rate = if total_windows > 0 {
        (total_windows - signals_generated) as f64 / total_windows as f64
    } else {
        0.0
    };

    BacktestResult {
        total_windows,
        signals_generated,
        correct,
        wrong,
        win_rate,
        skip_rate,
        details,
    }
}

#[derive(Debug)]
pub struct BacktestResult {
    pub total_windows: u32,
    pub signals_generated: u32,
    pub correct: u32,
    pub wrong: u32,
    pub win_rate: f64,
    pub skip_rate: f64,
    pub details: Vec<BacktestEntry>,
}

#[derive(Debug)]
pub struct BacktestEntry {
    pub time: i64,
    pub predicted: Direction,
    pub actual: Direction,
    pub confidence: f64,
    pub price_at_signal: f64,
    pub window_open: f64,
    pub window_close: f64,
    pub correct: bool,
}
