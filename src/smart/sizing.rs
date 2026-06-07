use super::SignalConfidence;

pub const DEFAULT_KELLY_FRACTION: f64 = 0.25;

#[derive(Clone, Copy, Debug)]
pub struct KellyInput {
    pub win_probability: f64,
    pub market_price: f64,
    pub bankroll: f64,
    pub max_per_trade: f64,
    pub remaining_budget: f64,
    pub fraction: f64,
}

pub fn probability_from_signal_confidence(confidence: &SignalConfidence) -> f64 {
    match confidence {
        SignalConfidence::Low => 0.53,
        SignalConfidence::Medium => 0.58,
        SignalConfidence::High => 0.65,
    }
}

pub fn fractional_kelly_fraction(win_probability: f64, market_price: f64, fraction: f64) -> f64 {
    if !win_probability.is_finite()
        || !market_price.is_finite()
        || !fraction.is_finite()
        || market_price <= 0.0
        || market_price >= 1.0
        || win_probability <= 0.0
        || win_probability >= 1.0
        || fraction <= 0.0
    {
        return 0.0;
    }

    let net_odds = (1.0 - market_price) / market_price;
    if net_odds <= 0.0 {
        return 0.0;
    }

    let loss_probability = 1.0 - win_probability;
    let full_kelly = (win_probability * net_odds - loss_probability) / net_odds;
    (full_kelly * fraction).clamp(0.0, 1.0)
}

pub fn fractional_kelly_position_size(input: KellyInput) -> f64 {
    if !input.bankroll.is_finite()
        || !input.max_per_trade.is_finite()
        || !input.remaining_budget.is_finite()
        || input.bankroll <= 0.0
        || input.max_per_trade <= 0.0
        || input.remaining_budget <= 0.0
    {
        return 0.0;
    }

    let kelly_fraction =
        fractional_kelly_fraction(input.win_probability, input.market_price, input.fraction);
    let raw_size = input.bankroll * kelly_fraction;
    raw_size
        .min(input.max_per_trade)
        .min(input.remaining_budget)
        .max(0.0)
}

pub fn smart_money_position_size(
    confidence: &SignalConfidence,
    market_price: f64,
    bankroll: f64,
    max_per_trade: f64,
    remaining_budget: f64,
) -> f64 {
    fractional_kelly_position_size(KellyInput {
        win_probability: probability_from_signal_confidence(confidence),
        market_price,
        bankroll,
        max_per_trade,
        remaining_budget,
        fraction: DEFAULT_KELLY_FRACTION,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_when_market_price_is_invalid() {
        assert_eq!(fractional_kelly_fraction(0.6, 0.0, 0.25), 0.0);
        assert_eq!(fractional_kelly_fraction(0.6, 1.0, 0.25), 0.0);
        assert_eq!(fractional_kelly_fraction(0.6, f64::NAN, 0.25), 0.0);
    }

    #[test]
    fn zero_when_estimated_edge_is_negative() {
        assert_eq!(fractional_kelly_fraction(0.55, 0.70, 0.25), 0.0);
    }

    #[test]
    fn quarter_kelly_fraction_for_even_money() {
        let fraction = fractional_kelly_fraction(0.60, 0.50, 0.25);
        assert!((fraction - 0.05).abs() < 1e-9);
    }

    #[test]
    fn position_size_respects_trade_and_budget_caps() {
        let size = fractional_kelly_position_size(KellyInput {
            win_probability: 0.75,
            market_price: 0.50,
            bankroll: 1_000.0,
            max_per_trade: 10.0,
            remaining_budget: 7.0,
            fraction: 0.25,
        });
        assert_eq!(size, 7.0);
    }

    #[test]
    fn smart_money_confidence_maps_to_nonzero_size_when_price_is_favorable() {
        let size = smart_money_position_size(&SignalConfidence::High, 0.50, 100.0, 10.0, 50.0);
        assert!(size > 0.0);
        assert!(size <= 10.0);
    }
}
