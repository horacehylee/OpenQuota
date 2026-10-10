use serde_json::{json, Value};

use crate::{
    models::{AppSettings, ProviderCatalog, QuotaFormat, QuotaWindow, UsagePeriod},
    service::UsageViewState,
};

pub fn usage_json(state: &UsageViewState) -> String {
    let mut value = serde_json::to_value(state).expect("usage state must serialize");
    inject_quota_remaining(&mut value);
    serde_json::to_string_pretty(&value).expect("usage state must serialize")
}

/// Adds `remaining = limit - used` to every quota window that reports both values so
/// terminal consumers do not re-derive it. Percent-only windows stay untouched.
fn inject_quota_remaining(state: &mut Value) {
    let Some(providers) = state.get_mut("providers").and_then(Value::as_object_mut) else {
        return;
    };
    for provider in providers.values_mut() {
        let Some(quotas) = provider
            .get_mut("snapshot")
            .and_then(Value::as_object_mut)
            .and_then(|snapshot| snapshot.get_mut("quotas"))
            .and_then(Value::as_array_mut)
        else {
            continue;
        };
        for quota in quotas {
            let Some(quota_object) = quota.as_object_mut() else {
                continue;
            };
            let used = quota_object.get("usedValue").and_then(Value::as_f64);
            let limit = quota_object.get("limitValue").and_then(Value::as_f64);
            if let (Some(used), Some(limit)) = (used, limit) {
                quota_object.insert(
                    "remaining".to_owned(),
                    json!(if limit > used { limit - used } else { 0.0 }),
                );
            }
        }
    }
}

pub fn usage_table(state: &UsageViewState, provider_ids: &[String]) -> String {
    let mut lines = Vec::new();
    for provider_id in provider_ids {
        let Some(view) = state.providers.get(provider_id) else {
            continue;
        };
        match &view.snapshot {
            Some(snapshot) => {
                lines.push(format!("{provider_id}:"));
                for quota in &snapshot.quotas {
                    lines.push(format!("  {} {}", quota.label, describe_quota(quota)));
                }
                for period in [
                    ("today", &snapshot.usage.today),
                    ("yesterday", &snapshot.usage.yesterday),
                    ("last 30 days", &snapshot.usage.last_30_days),
                ] {
                    if let Some(usage) = period.1 {
                        lines.push(format!("  {}: {}", period.0, describe_usage(usage)));
                    }
                }
            }
            None => lines.push(format!("{provider_id}: no snapshot")),
        }
        if let Some(error) = &view.error {
            lines.push(format!("  error: {error}"));
        }
    }
    if lines.is_empty() {
        lines.push("No enabled providers with data.".to_owned());
    }
    lines.join("\n")
}

fn describe_quota(quota: &QuotaWindow) -> String {
    match quota.format {
        QuotaFormat::Percent => format!("{:.1}% used", quota.used_percent),
        QuotaFormat::Dollars | QuotaFormat::Count => {
            let used = quota.used_value.unwrap_or(quota.used_percent);
            let unit = quota.unit.as_deref().unwrap_or("");
            match quota.limit_value {
                Some(limit) if unit.is_empty() => format!("{used:.2} of {limit:.2} used"),
                Some(limit) => format!("{used:.2} of {limit:.2} {unit} used"),
                None if unit.is_empty() => format!("{used:.2} used"),
                None => format!("{used:.2} {unit} used"),
            }
        }
    }
}

fn describe_usage(usage: &UsagePeriod) -> String {
    match usage.estimated_cost_usd {
        Some(cost) => format!("{} tokens, ${cost:.4}", usage.tokens),
        None => format!("{} tokens", usage.tokens),
    }
}

pub fn providers_json(catalog: &ProviderCatalog, settings: &AppSettings) -> String {
    let providers: Vec<Value> = catalog
        .providers
        .iter()
        .map(|definition| {
            let layout = settings
                .providers
                .iter()
                .find(|provider| provider.id == definition.id);
            json!({
                "id": definition.id,
                "displayName": definition.display_name,
                "enabled": layout.is_some_and(|provider| provider.enabled),
                "detected": layout.is_some_and(|provider| provider.detected),
            })
        })
        .collect();
    serde_json::to_string_pretty(&json!({ "providers": providers }))
        .expect("provider list must serialize")
}

pub fn providers_table(catalog: &ProviderCatalog, settings: &AppSettings) -> String {
    let mut lines = Vec::new();
    for definition in &catalog.providers {
        let layout = settings
            .providers
            .iter()
            .find(|provider| provider.id == definition.id);
        let enabled = layout.is_some_and(|provider| provider.enabled);
        let detected = layout.is_some_and(|provider| provider.detected);
        lines.push(format!(
            "{:<16} {:<12} enabled={} detected={}",
            definition.id, definition.display_name, enabled, detected
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::Utc;

    use super::*;
    use crate::models::{ProviderSnapshot, ProviderViewState, SnapshotSource, UsageHistory};

    fn snapshot(provider_id: &str, quotas: Vec<QuotaWindow>) -> ProviderSnapshot {
        ProviderSnapshot {
            provider_id: provider_id.to_owned(),
            plan: Some("Plus".to_owned()),
            quotas,
            usage: UsageHistory::default(),
            refreshed_at: Utc::now(),
            value_metrics: Vec::new(),
            status_metrics: Vec::new(),
            notices: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn view_state(snapshot: ProviderSnapshot) -> UsageViewState {
        let mut providers = BTreeMap::new();
        providers.insert(
            snapshot.provider_id.clone(),
            ProviderViewState {
                snapshot: Some(snapshot),
                source: SnapshotSource::Live,
                ..ProviderViewState::default()
            },
        );
        UsageViewState {
            providers,
            last_full_refresh_at: Some(Utc::now()),
        }
    }

    fn count_quota() -> QuotaWindow {
        QuotaWindow {
            id: "codex.primary".to_owned(),
            label: "Primary".to_owned(),
            used_percent: 60.0,
            resets_at: None,
            period_seconds: 5 * 60 * 60,
            format: QuotaFormat::Count,
            used_value: Some(3_000_000.0),
            limit_value: Some(5_000_000.0),
            unit: Some("tokens".to_owned()),
            estimated: false,
            source_note: None,
        }
    }

    #[test]
    fn usage_json_is_camel_case_and_reports_remaining() {
        let snapshot = snapshot(
            "codex",
            vec![
                count_quota(),
                QuotaWindow {
                    used_value: None,
                    limit_value: None,
                    unit: None,
                    ..count_quota()
                },
            ],
        );
        let value: Value =
            serde_json::from_str(&usage_json(&view_state(snapshot))).expect("valid JSON");

        assert!(value.get("lastFullRefreshAt").is_some());
        let codex = &value["providers"]["codex"];
        assert_eq!(codex["snapshot"]["plan"], "Plus");
        let quotas = codex["snapshot"]["quotas"].as_array().expect("quotas");
        assert_eq!(quotas[0]["remaining"], json!(2_000_000.0));
        assert!(quotas[1].get("remaining").is_none());
    }

    #[test]
    fn remaining_never_goes_negative_when_usage_exceeds_limit() {
        let quota = QuotaWindow {
            used_value: Some(6.0),
            limit_value: Some(5.0),
            ..count_quota()
        };
        let value: Value =
            serde_json::from_str(&usage_json(&view_state(snapshot("codex", vec![quota]))))
                .expect("valid JSON");
        assert_eq!(
            value["providers"]["codex"]["snapshot"]["quotas"][0]["remaining"],
            json!(0.0)
        );
    }

    #[test]
    fn usage_table_lists_quotas_and_usage() {
        let mut snapshot = snapshot("codex", vec![count_quota()]);
        snapshot.usage = UsageHistory {
            today: Some(crate::models::UsagePeriod {
                tokens: 1234,
                estimated_cost_usd: Some(0.5),
                cost_estimated: true,
                estimate_complete: false,
                model_breakdown: None,
                unknown_models: Vec::new(),
            }),
            ..UsageHistory::default()
        };
        let text = usage_table(&view_state(snapshot), &["codex".to_owned()]);
        assert!(text.contains("codex:"));
        assert!(text.contains("Primary 3000000.00 of 5000000.00 tokens used"));
        assert!(text.contains("today: 1234 tokens, $0.5000"));
    }

    #[test]
    fn inject_quota_remaining_tolerates_missing_fields() {
        let mut value = json!({ "providers": {} });
        inject_quota_remaining(&mut value);
        assert_eq!(value, json!({ "providers": {} }));
    }
}
