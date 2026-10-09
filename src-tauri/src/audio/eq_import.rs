//! AutoEq text-format import. AutoEq (the community headphone-correction
//! project) publishes parametric EQs as plain text:
//!
//! ```text
//! Preamp: -6.0 dB
//! Filter 1: ON PK Fc 105 Hz Gain -2.4 dB Q 0.70
//! ```
//!
//! Tolerant token parsing, no regex: disabled and unrecognized filter
//! lines are skipped (a partial import beats a hard failure over one
//! exotic filter type); it errors only when nothing usable remains.

use crate::audio::types::{EqBand, EqBandKind, EqConfig, MAX_EQ_BANDS};
use crate::error::SinkError;

fn kind_from_token(token: &str) -> Option<EqBandKind> {
    match token {
        "PK" | "PEQ" | "Modal" => Some(EqBandKind::Peaking),
        "LS" | "LSC" => Some(EqBandKind::LowShelf),
        "HS" | "HSC" => Some(EqBandKind::HighShelf),
        "LP" | "LPQ" => Some(EqBandKind::LowPass),
        "HP" | "HPQ" => Some(EqBandKind::HighPass),
        _ => None,
    }
}

/// Value following a `label` token (e.g. "Fc" -> 105.0 from "Fc 105 Hz").
fn value_after(tokens: &[&str], label: &str) -> Option<f32> {
    tokens
        .iter()
        .position(|t| t.eq_ignore_ascii_case(label))
        .and_then(|i| tokens.get(i + 1))
        .and_then(|v| v.parse().ok())
}

fn parse_filter_line(line: &str) -> Option<EqBand> {
    // "Filter N: ON PK Fc 105 Hz Gain -2.4 dB Q 0.70"
    let rest = line.split(':').nth(1)?.trim();
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    if tokens.first().map(|t| t.eq_ignore_ascii_case("ON")) != Some(true) {
        return None; // disabled ("OFF") or malformed
    }
    let kind = kind_from_token(tokens.get(1)?)?;
    let freq_hz = value_after(&tokens, "Fc")?;
    let gain_db = value_after(&tokens, "Gain").unwrap_or(0.0);
    // Shelf lines in AutoEq's fixed-band output often omit Q.
    let q = value_after(&tokens, "Q").unwrap_or(match kind {
        EqBandKind::LowShelf | EqBandKind::HighShelf => 0.71,
        _ => 1.0,
    });
    let mut band = EqBand {
        kind,
        freq_hz,
        gain_db,
        q,
    };
    band.clamp_ranges();
    Some(band)
}

/// Parse an AutoEq result block into a (disabled, preview-ready) EqConfig.
/// Keeps the first MAX_EQ_BANDS filters in file order - AutoEq emits them
/// in descending importance already.
pub fn parse_autoeq(text: &str) -> Result<EqConfig, SinkError> {
    let mut preamp_db = 0.0f32;
    let mut bands: Vec<EqBand> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.to_ascii_lowercase().starts_with("preamp") {
            if let Some(v) = line
                .split(':')
                .nth(1)
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|v| v.parse::<f32>().ok())
            {
                preamp_db = v;
            }
        } else if line.to_ascii_lowercase().starts_with("filter") && bands.len() < MAX_EQ_BANDS {
            if let Some(band) = parse_filter_line(line) {
                bands.push(band);
            }
        }
    }
    if bands.is_empty() {
        return Err(SinkError::Parse(
            "no usable filters found (expected AutoEq lines like \
             'Filter 1: ON PK Fc 105 Hz Gain -2.4 dB Q 0.70')"
                .into(),
        ));
    }
    let mut config = EqConfig {
        enabled: false,
        preamp_db,
        bands,
        ..EqConfig::default()
    };
    config.clamp_ranges();
    Ok(config)
}
