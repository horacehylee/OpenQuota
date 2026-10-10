use chrono::{DateTime, Days, NaiveDate, Utc};
use serde_json::Value;

use crate::{
    models::UsageHistory,
    pricing::{ModelPricing, TokenBreakdown},
};

use super::ZaiError;

pub(super) const USAGE_SOURCE_NOTE: &str = "From your Z.ai model usage (estimated)";

/// Maps the `usage-detail` response into a `UsageHistory`, pricing tokens at the
/// first-party `zai/<modelCode>` catalog rates. Buckets are Z.ai days, so the
/// caller passes the same `start_date` used to build the request window.
pub(super) fn map_usage_detail(
    body: &Value,
    start_date: NaiveDate,
    now: DateTime<Utc>,
    pricing: &ModelPricing,
) -> Result<UsageHistory, ZaiError> {
    let usage = body
        .get("data")
        .and_then(|data| data.get("modelUsage"))
        .ok_or(ZaiError::InvalidResponse)?;
    let models = usage
        .get("modelDataList")
        .and_then(Value::as_array)
        .ok_or(ZaiError::InvalidResponse)?;
    // The server pads one day before `startTime`, so bucket dates come from
    // `xTime` when present, otherwise fall back to the observed alignment.
    let bucket_dates: Vec<Option<NaiveDate>> = usage
        .get("xTime")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .and_then(|text| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok())
                })
                .collect()
        })
        .unwrap_or_default();

    let mut accumulator = crate::providers::daily_usage::DailyUsageAccumulator::default();
    for model in models {
        let name = model
            .get("modelCode")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .ok_or(ZaiError::InvalidResponse)?;
        let uncached = token_series(model.get("uncachedInputTokensUsage"));
        let cached = token_series(model.get("cachedInputTokensUsage"));
        let output = token_series(model.get("outputTokensUsage"));
        let total = token_series(model.get("totalTokensUsage"));

        for (day_offset, tokens) in total.iter().copied().enumerate() {
            if tokens == 0 {
                continue;
            }
            let date = match bucket_dates.get(day_offset).copied().flatten() {
                Some(date) => Some(date),
                None => start_date
                    .checked_sub_days(Days::new(1))
                    .and_then(|first| first.checked_add_days(Days::new(day_offset as u64))),
            };
            let Some(date) = date else {
                return Err(ZaiError::InvalidResponse);
            };
            let breakdown = TokenBreakdown {
                input: uncached.get(day_offset).copied().unwrap_or(0),
                cache_write_5m: 0,
                cache_write_1h: 0,
                cache_read: cached.get(day_offset).copied().unwrap_or(0),
                output: output.get(day_offset).copied().unwrap_or(0),
                is_fast: false,
            };
            let cost = pricing.estimated_cost_dollars(&format!("zai/{name}"), breakdown, false);
            match cost {
                Some(cost) => accumulator.add(date, tokens, cost, name),
                None => accumulator.add_unknown_model(date, name),
            }
        }
    }

    Ok(accumulator.build(now, USAGE_SOURCE_NOTE))
}

fn token_series(value: Option<&Value>) -> Vec<u64> {
    value
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .map(|value| value.as_u64().unwrap_or(0))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::TimeZone;
    use serde_json::json;

    use crate::pricing::{ModelPricing, ModelRates, PricingCatalog, PricingSupplement};

    use super::*;

    fn envelope(models: Value, x_time: Value) -> Value {
        json!({"data": {"modelUsage": {"modelDataList": models, "xTime": x_time}}})
    }

    fn pricing() -> ModelPricing {
        let mut entries = HashMap::new();
        entries.insert("zai/glm-5.3".to_owned(), ModelRates::new(2.0, 4.0));
        ModelPricing::new(
            PricingSupplement::default(),
            PricingCatalog {
                entries,
                retrieved_at: None,
            },
            PricingCatalog::default(),
        )
    }

    #[test]
    fn usage_detail_maps_tokens_and_zai_pricing_per_day() {
        let body = envelope(
            json!([
                {
                    "modelCode": "glm-5.3",
                    "uncachedInputTokensUsage": [1_000_000, 0, 400_000],
                    "cachedInputTokensUsage": [1_000_000, 0, 100_000],
                    "inputTokensUsage": [2_000_000, 0, 500_000],
                    "outputTokensUsage": [250_000, 0, 0],
                    "totalTokensUsage": [2_250_000, 0, 500_000]
                },
                {
                    "modelCode": "glm-5.3-flash",
                    "uncachedInputTokensUsage": [0, 0, 0],
                    "cachedInputTokensUsage": [0, 0, 0],
                    "outputTokensUsage": [0, 0, 0],
                    "totalTokensUsage": [0, 0, 0]
                }
            ]),
            json!(["2026-10-04", "2026-10-05", "2026-10-06"]),
        );

        let start = NaiveDate::from_ymd_opt(2026, 10, 4).unwrap();
        // 2026-10-06T12:00Z = Oct 6 local (UTC+8), so bucket 2 is "today".
        let now = Utc
            .timestamp_millis_opt(1_791_290_400_000)
            .single()
            .unwrap();
        let history = map_usage_detail(&body, start, now, &pricing()).unwrap();

        let daily = &history.daily;
        assert_eq!(daily.len(), 2);
        // Newest first: Oct 6 (today), then Oct 4.
        assert_eq!(daily[1].tokens, 2_250_000);
        assert_eq!(daily[1].estimated_cost_usd, Some(3.2));
        assert_eq!(daily[0].tokens, 500_000);
        // 400k uncached * 2.0 + 100k cached * 0.2 = 0.82.
        assert_eq!(daily[0].estimated_cost_usd, Some(0.82));

        let today = history.today.as_ref().unwrap();
        assert_eq!(today.tokens, 500_000);
        assert_eq!(today.estimated_cost_usd, Some(0.82));
        assert!(today.cost_estimated);

        let breakdown = history
            .last_30_days
            .as_ref()
            .unwrap()
            .model_breakdown
            .as_ref()
            .unwrap();
        let glm = breakdown
            .models
            .iter()
            .find(|model| model.model == "glm-5.3")
            .unwrap();
        assert_eq!(glm.total_tokens, 2_750_000);
    }

    #[test]
    fn unpriced_models_keep_tokens_without_cost() {
        let body = envelope(
            json!([{
                "modelCode": "glm-9.9",
                "uncachedInputTokensUsage": [10],
                "cachedInputTokensUsage": [0],
                "inputTokensUsage": [10],
                "outputTokensUsage": [5],
                "totalTokensUsage": [15]
            }]),
            json!(["2026-10-04"]),
        );

        let start = NaiveDate::from_ymd_opt(2026, 10, 4).unwrap();
        let now = Utc
            .timestamp_millis_opt(1_792_230_192_983)
            .single()
            .unwrap();
        let history = map_usage_detail(&body, start, now, &pricing()).unwrap();

        assert!(history.daily.is_empty());
        assert_eq!(history.unknown_models, vec!["glm-9.9".to_owned()]);
    }

    #[test]
    fn missing_envelope_is_invalid_response() {
        let start = NaiveDate::from_ymd_opt(2026, 10, 4).unwrap();
        let now = Utc
            .timestamp_millis_opt(1_792_230_192_983)
            .single()
            .unwrap();
        for body in [
            json!({}),
            json!({"data":{}}),
            json!({"data":{"modelUsage":{}}}),
            json!({"data":{"modelDataList":{}}}),
            json!({"data":{"modelUsage":{"modelDataList":{}}}}),
        ] {
            assert!(map_usage_detail(&body, start, now, &pricing()).is_err());
        }
    }

    #[test]
    fn live_thirty_day_payload_maps_real_dates() {
        let body: Value =
            serde_json::from_str(include_str!("fixtures/usage_detail_30d_live.json")).unwrap();
        let start = NaiveDate::from_ymd_opt(2026, 9, 11).unwrap();
        // 2026-10-10T12:00Z = Oct 10 local (UTC+8); last xTime bucket is today.
        let now = Utc
            .timestamp_millis_opt(1_791_624_000_000)
            .single()
            .unwrap();
        let history = map_usage_detail(&body, start, now, &pricing()).unwrap();

        assert!(!history.daily.is_empty());
        let today = history.today.as_ref().unwrap();
        // glm-5.3-flash is the only model with usage on the final bucket.
        assert_eq!(today.tokens, 32_503_516);
        let breakdown = history
            .last_30_days
            .as_ref()
            .unwrap()
            .model_breakdown
            .as_ref()
            .unwrap();
        let flash = breakdown
            .models
            .iter()
            .find(|model| model.model == "glm-5.3-flash")
            .unwrap();
        assert_eq!(flash.total_tokens, 265_061_874);
    }
}
