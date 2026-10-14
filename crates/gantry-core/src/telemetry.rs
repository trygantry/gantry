use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ObservedTelemetry {
    pub source: String,
    pub environment: String,
    pub release: String,
    pub crash_free_rate: Option<f64>,
    #[serde(default)]
    pub metrics: HashMap<String, f64>,
    pub fetched_at: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TelemetryObsMap {
    #[serde(default)]
    pub observations: HashMap<String, ObservedTelemetry>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryRule {
    pub metric: String,
    pub comparator: String, // "<" | "<=" | ">" | ">=" | "==" | "!="
    pub threshold: f64,
    pub severity: String, // "required" | "warning"
}

pub fn evaluate_telemetry_rules(
    rules: &[TelemetryRule],
    telemetry: &ObservedTelemetry,
    readiness_reasons: &mut Vec<String>,
    mon_reasons: &mut Vec<String>,
    monitoring_score: &mut i32,
    readiness: &mut String,
) {
    // Normalization: Ensure crash_free_rate is mapped to metrics dictionary
    let mut metrics = telemetry.metrics.clone();
    if let Some(cf_rate) = telemetry.crash_free_rate {
        metrics
            .entry("sentry.crash_free_rate".to_string())
            .or_insert(cf_rate);
    }

    for rule in rules {
        if let Some(&val) = metrics.get(&rule.metric) {
            let passed = match rule.comparator.as_str() {
                "<" => val < rule.threshold,
                "<=" => val <= rule.threshold,
                ">" => val > rule.threshold,
                ">=" => val >= rule.threshold,
                "==" => (val - rule.threshold).abs() < 1e-9,
                "!=" => (val - rule.threshold).abs() >= 1e-9,
                _ => false,
            };

            if !passed {
                let msg = format!(
                    "Telemetry metric '{}' value ({}) does not satisfy comparator '{}' threshold ({}).",
                    rule.metric, val, rule.comparator, rule.threshold
                );
                readiness_reasons.push(msg.clone());
                mon_reasons.push(format!("-100: {}", msg));

                if rule.severity.to_lowercase() == "required" {
                    *readiness = "No-Go".to_string();
                    *monitoring_score = 0;
                } else {
                    if *readiness == "Go" {
                        *readiness = "At Risk".to_string();
                    }
                    *monitoring_score = (*monitoring_score - 50).max(0);
                }
            }
        } else {
            let msg = format!(
                "Telemetry metric '{}' is missing from observations.",
                rule.metric
            );
            readiness_reasons.push(msg.clone());
            mon_reasons.push(format!("-100: {}", msg));

            if rule.severity.to_lowercase() == "required" {
                *readiness = "No-Go".to_string();
                *monitoring_score = 0;
            } else {
                if *readiness == "Go" {
                    *readiness = "At Risk".to_string();
                }
                *monitoring_score = (*monitoring_score - 50).max(0);
            }
        }
    }
}
