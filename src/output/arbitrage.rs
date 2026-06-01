use tabled::settings::Style;
use tabled::{Table, Tabled};
use crate::output::{format_decimal, truncate};
use crate::arbitrage::{ArbOpportunity, FLBOpportunity};

#[derive(Tabled)]
struct ComplementRow {
    #[tabled(rename = "Question")]
    question: String,
    #[tabled(rename = "YES Ask")]
    yes_ask: String,
    #[tabled(rename = "NO Ask")]
    no_ask: String,
    #[tabled(rename = "Sum Price")]
    sum_price: String,
    #[tabled(rename = "Profit Margin")]
    profit_margin: String,
    #[tabled(rename = "Volume")]
    volume: String,
}

#[derive(Tabled)]
struct BiasRow {
    #[tabled(rename = "Question")]
    question: String,
    #[tabled(rename = "Outcome")]
    outcome: String,
    #[tabled(rename = "Price")]
    price: String,
    #[tabled(rename = "Bias Type")]
    bias_type: String,
    #[tabled(rename = "Volume")]
    volume: String,
}

pub fn print_complement_table(opps: &[ArbOpportunity]) {
    if opps.is_empty() {
        println!("No complement arbitrage opportunities found. All YES/NO sums are >= $1.00.");
        return;
    }

    let rows: Vec<ComplementRow> = opps
        .iter()
        .map(|o| ComplementRow {
            question: truncate(&o.question, 50),
            yes_ask: format!("${:.3}", o.yes_ask),
            no_ask: format!("${:.3}", o.no_ask),
            sum_price: format!("${:.3}", o.sum_price),
            profit_margin: format!("+{:+.2}%", o.profit_margin_pct),
            volume: format_decimal(o.volume),
        })
        .collect();

    let table = Table::new(rows).with(Style::rounded()).to_string();
    println!("{table}");
}

pub fn print_bias_table(opps: &[FLBOpportunity]) {
    if opps.is_empty() {
        println!("No Favorite-Longshot Bias opportunities found.");
        return;
    }

    let rows: Vec<BiasRow> = opps
        .iter()
        .map(|o| BiasRow {
            question: truncate(&o.question, 50),
            outcome: o.outcome.clone(),
            price: format!("${:.2}", o.price),
            bias_type: o.bias_type.clone(),
            volume: format_decimal(o.volume),
        })
        .collect();

    let table = Table::new(rows).with(Style::rounded()).to_string();
    println!("{table}");
}
