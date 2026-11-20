//! Notification delivery for a live search: prepared secrets, an off-thread worker and the planner.
//! Delivery never runs on the browser thread; sending never reads the native vault.
use ceh_services::notifications::{
    DeliveryIntent, DeliveryOutcome, NotificationError, NotificationPlanner, NotificationStateInput,
    PreparedNotifications, notification_state, prepare_notifications,
};
use ceh_services::secrets::{Backend, Vault};
use serde_json::Value;
use std::{path::Path, sync::mpsc, thread::JoinHandle};

/// Read the secrets of enabled channels before the browser opens. Unreadable secrets stop the start.
pub fn prepare_for_start<B: Backend>(
    root: &Path,
    profile_id: &str,
    vault: &Vault<B>,
) -> Result<(Value, PreparedNotifications), &'static str> {
    let config = crate::notification_editor::runtime_config(root, profile_id).map_err(|_| "profile_invalid")?;
    let prepared = prepare_notifications(profile_id, &config, vault, None)
        .map_err(|_| "notification_secrets_unavailable")?;
    Ok((config, prepared))
}

pub struct NotificationWorker {
    sender: Option<mpsc::Sender<Vec<DeliveryIntent>>>,
    thread: Option<JoinHandle<()>>,
}
impl NotificationWorker {
    /// `deliver` runs on the worker thread; `sink` receives every outcome (including `Stale`).
    pub fn spawn<D, S>(prepared: PreparedNotifications, deliver: D, sink: S) -> Self
    where
        D: Fn(&DeliveryIntent, &PreparedNotifications) -> Result<DeliveryOutcome, NotificationError> + Send + 'static,
        S: Fn(&'static str, Result<DeliveryOutcome, NotificationError>) + Send + 'static,
    {
        let (sender, receiver) = mpsc::channel::<Vec<DeliveryIntent>>();
        let thread = std::thread::Builder::new()
            .name("ceh-notifications".into())
            .spawn(move || {
                while let Ok(batch) = receiver.recv() {
                    for intent in batch {
                        let result = if intent.cancellation().current() {
                            deliver(&intent, &prepared)
                        } else {
                            Err(NotificationError::Stale)
                        };
                        sink(intent.channel(), result);
                    }
                }
            })
            .ok();
        Self { sender: Some(sender), thread }
    }
    pub fn submit(&self, intents: Vec<DeliveryIntent>) {
        if !intents.is_empty()
            && let Some(sender) = &self.sender
        {
            let _ = sender.send(intents);
        }
    }
}
impl Drop for NotificationWorker {
    /// Pending alerts are delivered before the worker ends.
    fn drop(&mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub struct LiveSettings {
    pub profile_id: String,
    pub label: String,
    pub province: String,
    pub language: String,
    pub config: Value,
    pub browser: String,
    pub run_id: String,
    /// The service as the person knows it, how they sign in (`certificate`, `movil`, `permanente`) and the profile's
    /// time zone: what a sign-in or stop message says besides the profile and the province. Empty when unknown.
    pub service: String,
    pub access: String,
    pub timezone: String,
}
/// Declared worker first: pending alerts are delivered before the planner cancels its intents.
pub struct LiveNotifications {
    worker: NotificationWorker,
    planner: NotificationPlanner,
    settings: LiveSettings,
}
impl LiveNotifications {
    pub fn start<D, S>(settings: LiveSettings, prepared: PreparedNotifications, deliver: D, sink: S) -> Self
    where
        D: Fn(&DeliveryIntent, &PreparedNotifications) -> Result<DeliveryOutcome, NotificationError> + Send + 'static,
        S: Fn(&'static str, Result<DeliveryOutcome, NotificationError>) + Send + 'static,
    {
        Self {
            worker: NotificationWorker::spawn(prepared, deliver, sink),
            planner: NotificationPlanner::default(),
            settings,
        }
    }
    /// Feed the current page facts; due alerts go to the worker.
    pub fn observe(&mut self, mut input: NotificationStateInput) {
        input.run_id = self.settings.run_id.clone();
        input.browser = Some(self.settings.browser.clone());
        let known = |value: &String| (!value.is_empty()).then(|| value.clone());
        input.service = input.service.or_else(|| known(&self.settings.service));
        input.access = input.access.or_else(|| known(&self.settings.access));
        if input.at.is_none() {
            input.at = clock_text(&self.settings.timezone);
        }
        let state = notification_state(&input);
        let settings = &self.settings;
        if let Ok(intents) = self.planner.observe(
            &settings.profile_id,
            &settings.label,
            &settings.province,
            &state,
            &settings.language,
            &settings.config,
        ) {
            self.worker.submit(intents);
        }
    }
}

/// The time of day in the profile's time zone (Madrid when it is not a known one), as `HH:MM`.
fn clock_text(timezone: &str) -> Option<String> {
    let zone = timezone.parse::<chrono_tz::Tz>().unwrap_or(chrono_tz::Europe::Madrid);
    crate::current_time().ok().map(|now| now.with_timezone(&zone).format("%H:%M").to_string())
}
/// The service of a profile as the person knows it.
pub fn service_label(procedure: &str) -> String {
    match procedure {
        "huellas" => "Toma de huellas / TIE",
        "regreso" => "Autorización de regreso",
        _ => "",
    }
    .to_owned()
}
/// How the person signs in, as the notification wording knows it.
pub fn access_label(access_mode: &str, clave_method: &str) -> String {
    match (access_mode, clave_method) {
        ("certificate", _) => "certificate".to_owned(),
        (_, "movil" | "permanente") => clave_method.to_owned(),
        _ => String::new(),
    }
}

/// How long the search has waited on one page for a person: counted from the first look at that page, started
/// again on another page and after any break (times are the search's own clock, in milliseconds).
#[derive(Default)]
pub struct HoldWatch {
    since: Option<(String, u64)>,
}
impl HoldWatch {
    /// `holding`: the page waits for a person. Returns the seconds waited so far, `None` when it is not waiting.
    pub fn update(&mut self, document: &str, holding: bool, now_ms: u64) -> Option<u64> {
        if !holding {
            self.since = None;
            return None;
        }
        match &self.since {
            Some((known, at)) if known == document => Some(now_ms.saturating_sub(*at) / 1000),
            _ => {
                self.since = Some((document.to_owned(), now_ms));
                Some(0)
            }
        }
    }
}
