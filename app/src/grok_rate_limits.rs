use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Local};
use futures::future::{Either, select};
use serde::Deserialize;
use warpui::r#async::Timer;
use warpui::{Entity, ModelContext, SingletonEntity};

const REFRESH_INTERVAL: Duration = Duration::from_secs(5 * 60);
const QUERY_TIMEOUT: Duration = Duration::from_secs(20);
const BILLING_URL: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";
const AUTH_SCOPE_PREFIX: &str = "https://auth.x.ai::";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GrokQuotaSnapshot {
    remaining_percent: u8,
    resets_at: Option<String>,
}

impl GrokQuotaSnapshot {
    pub(crate) fn remaining_percent(&self) -> u8 {
        self.remaining_percent
    }

    #[allow(dead_code)]
    pub(crate) fn resets_at(&self) -> Option<&str> {
        self.resets_at.as_deref()
    }
}

pub(crate) struct GrokRateLimitsModel {
    state: GrokQuotaState,
    started: bool,
    refresh_in_flight: bool,
    refresh_schedule_generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GrokQuotaState {
    Loading,
    Available(GrokQuotaSnapshot),
    Unavailable,
}

impl GrokQuotaState {
    fn label(&self) -> String {
        match self {
            Self::Loading => "Grok …".to_owned(),
            Self::Available(snapshot) => format!("Grok {}%", snapshot.remaining_percent()),
            Self::Unavailable => "Grok --".to_owned(),
        }
    }

    fn detail_label(&self) -> String {
        match self {
            Self::Loading => "Local Grok quota loading".to_owned(),
            Self::Available(snapshot) => {
                let reset = snapshot
                    .resets_at()
                    .and_then(format_reset_rfc3339)
                    .map(|reset| format!(" · resets {reset}"))
                    .unwrap_or_default();
                format!(
                    "Local Grok · {}% remaining{reset} · click to refresh",
                    snapshot.remaining_percent()
                )
            }
            Self::Unavailable => "Local Grok quota unavailable · click to retry".to_owned(),
        }
    }
}

impl GrokRateLimitsModel {
    pub(crate) fn new() -> Self {
        Self {
            state: GrokQuotaState::Loading,
            started: false,
            refresh_in_flight: false,
            refresh_schedule_generation: 0,
        }
    }

    pub(crate) fn ensure_started(&mut self, ctx: &mut ModelContext<Self>) {
        if self.started {
            return;
        }
        self.started = true;
        self.refresh_now(ctx);
    }

    pub(crate) fn label(&self) -> String {
        self.state.label()
    }

    pub(crate) fn detail_label(&self) -> String {
        self.state.detail_label()
    }

    pub(crate) fn refresh_now(&mut self, ctx: &mut ModelContext<Self>) {
        self.start_refresh(true, ctx);
    }

    fn start_refresh(&mut self, show_loading: bool, ctx: &mut ModelContext<Self>) {
        if self.refresh_in_flight {
            return;
        }

        // Invalidate the previously scheduled timer. Its callback is kept
        // intentionally cheap and will exit without issuing a request.
        self.refresh_schedule_generation = self.refresh_schedule_generation.wrapping_add(1);
        self.refresh_in_flight = true;
        if show_loading && self.state != GrokQuotaState::Loading {
            self.state = GrokQuotaState::Loading;
            ctx.notify();
            ctx.emit(GrokRateLimitsEvent::Updated);
        }

        let _ = ctx.spawn(
            query_grok_rate_limits_with_timeout(),
            |model, result, ctx| {
                model.refresh_in_flight = false;
                let next_state = match result {
                    Ok(snapshot) => GrokQuotaState::Available(snapshot),
                    Err(error) => {
                        if !matches!(model.state, GrokQuotaState::Unavailable) {
                            log::warn!("Grok rate-limit query unavailable: {error:#}");
                        } else {
                            log::debug!("Grok rate-limit query still unavailable: {error:#}");
                        }
                        GrokQuotaState::Unavailable
                    }
                };

                if model.state != next_state {
                    model.state = next_state;
                    ctx.notify();
                    ctx.emit(GrokRateLimitsEvent::Updated);
                }

                model.schedule_refresh(ctx);
            },
        );
    }

    fn schedule_refresh(&mut self, ctx: &mut ModelContext<Self>) {
        self.refresh_schedule_generation = self.refresh_schedule_generation.wrapping_add(1);
        let generation = self.refresh_schedule_generation;
        let _ = ctx.spawn(Timer::after(REFRESH_INTERVAL), move |model, _, ctx| {
            if model.refresh_schedule_generation == generation {
                model.start_refresh(false, ctx);
            }
        });
    }
}

fn format_reset_rfc3339(resets_at: &str) -> Option<String> {
    DateTime::parse_from_rfc3339(resets_at).ok().map(|reset| {
        reset
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M %Z")
            .to_string()
    })
}

#[derive(Debug)]
pub(crate) enum GrokRateLimitsEvent {
    Updated,
}

impl Entity for GrokRateLimitsModel {
    type Event = GrokRateLimitsEvent;
}

impl SingletonEntity for GrokRateLimitsModel {}

async fn query_grok_rate_limits_with_timeout() -> Result<GrokQuotaSnapshot> {
    let query = Box::pin(query_grok_rate_limits());
    let timeout = Box::pin(Timer::after(QUERY_TIMEOUT));
    match select(query, timeout).await {
        Either::Left((result, _)) => result,
        Either::Right((_, _)) => Err(anyhow!("Grok rate-limit query timed out")),
    }
}

async fn query_grok_rate_limits() -> Result<GrokQuotaSnapshot> {
    let auth = load_grok_auth()?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .context("failed to construct Grok billing client")?;
    let response = client
        .get(BILLING_URL)
        .bearer_auth(&auth.access_token)
        .header("X-XAI-Token-Auth", "xai-grok-cli")
        .header("x-userid", &auth.user_id)
        .send()
        .await
        .context("failed to query Grok billing service")?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Grok billing service returned HTTP {}",
            response.status().as_u16()
        ));
    }

    let response = response
        .json::<BillingResponse>()
        .await
        .context("invalid Grok billing response")?;
    snapshot_from_response(response)
}

fn load_grok_auth() -> Result<GrokAuthRecord> {
    let auth_path = dirs::home_dir()
        .context("home directory unavailable")?
        .join(".grok/auth.json");
    let contents = std::fs::read_to_string(&auth_path)
        .with_context(|| format!("failed to read {}", auth_path.display()))?;
    let auth_by_scope = serde_json::from_str::<HashMap<String, GrokAuthRecord>>(&contents)
        .context("invalid Grok authentication file")?;

    auth_by_scope
        .into_iter()
        .find_map(|(scope, auth)| {
            (scope.starts_with(AUTH_SCOPE_PREFIX)
                && !auth.access_token.trim().is_empty()
                && !auth.user_id.trim().is_empty())
            .then_some(auth)
        })
        .context("no signed-in Grok account found")
}

fn snapshot_from_response(response: BillingResponse) -> Result<GrokQuotaSnapshot> {
    let config = response
        .config
        .context("Grok billing response had no config")?;
    let limit = config.monthly_limit.map(|value| value.val).unwrap_or(0);
    let used = config.used.map(|value| value.val).unwrap_or(0);
    let usage_percent = match config.credit_usage_percent {
        Some(percent) => percent.clamp(0.0, 100.0),
        None if limit > 0 => (used as f64 / limit as f64 * 100.0).clamp(0.0, 100.0),
        None => 0.0,
    };
    let rounded_usage = usage_percent.round() as i32;

    Ok(GrokQuotaSnapshot {
        remaining_percent: 100_i32.saturating_sub(rounded_usage) as u8,
        resets_at: config.current_period.and_then(|period| period.end),
    })
}

#[derive(Deserialize)]
struct GrokAuthRecord {
    #[serde(rename = "key")]
    access_token: String,
    user_id: String,
}

#[derive(Deserialize)]
struct BillingResponse {
    config: Option<BillingConfig>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BillingConfig {
    credit_usage_percent: Option<f64>,
    monthly_limit: Option<Cent>,
    used: Option<Cent>,
    current_period: Option<UsagePeriod>,
}

#[derive(Deserialize)]
struct Cent {
    #[serde(default)]
    val: i64,
}

#[derive(Deserialize)]
struct UsagePeriod {
    end: Option<String>,
}

#[cfg(test)]
#[path = "grok_rate_limits_tests.rs"]
mod tests;
