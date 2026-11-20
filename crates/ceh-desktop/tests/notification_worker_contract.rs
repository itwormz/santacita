use ceh_core::model::PageKind;
use ceh_desktop::notification_worker::{HoldWatch, LiveNotifications, LiveSettings, NotificationWorker, prepare_for_start};
use ceh_services::notifications::{
    DeliveryOutcome, NotificationError, NotificationPlanner, NotificationStateInput, prepare_notifications,
};
use ceh_services::secrets::{Backend, Vault, VaultError};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

fn office(document: &str, attention: bool) -> NotificationStateInput {
    NotificationStateInput {
        document: Some(document.into()),
        page: PageKind::Office,
        attention,
        offices: vec!["Oficina A".into(), "Oficina B".into()],
        ..Default::default()
    }
}
fn config() -> Value {
    json!({"telegram":{"enabled":true,"events":["office","booking"],"recipients":[{"id":"1"}],"secret_ref":""},
           "webhook":{"enabled":true,"events":["error"],"secret_ref":""}})
}
fn settings() -> LiveSettings {
    LiveSettings {
        profile_id: "p".into(),
        label: "Demo".into(),
        province: "Madrid".into(),
        language: "en".into(),
        config: config(),
        browser: "Firefox".into(),
        run_id: "run-1".into(),
        service: String::new(),
        access: String::new(),
        timezone: String::new(),
    }
}
type Sent = Arc<Mutex<Vec<(String, String)>>>;
fn live(sent: &Sent) -> (LiveNotifications, mpsc::Receiver<()>) {
    let log = sent.clone();
    let prepared = prepare_notifications("p", &json!({}), &Vault::new(Locked::default()), None).unwrap();
    let (completed, receipt) = mpsc::channel();
    let notifications = LiveNotifications::start(
        settings(),
        prepared,
        move |intent, _| {
            log.lock().unwrap().push((intent.channel().into(), intent.message().into()));
            Ok(DeliveryOutcome::TelegramAccepted)
        },
        move |_, _| { let _ = completed.send(()); },
    );
    (notifications, receipt)
}

#[test]
fn one_office_alert_per_document_lists_offices_and_names_the_browser() {
    let sent: Sent = Default::default();
    let (mut notifications, completed) = live(&sent);
    notifications.observe(office("d1", true));
    // d2 intentionally invalidates queued d1 alerts. Confirm delivery before changing documents;
    // otherwise this deduplication test depends on which thread the OS schedules first.
    completed.recv_timeout(Duration::from_secs(5)).expect("first office alert completes");
    for step in [office("d1", true), office("d1", false), office("d1", true), office("d1", false), office("d1", true)] {
        notifications.observe(step);
    }
    notifications.observe(office("d2", true));
    drop(notifications);
    let sent = sent.lock().unwrap();
    assert_eq!(sent.len(), 2, "one alert for d1 and one for d2: {sent:?}");
    for (channel, message) in sent.iter() {
        assert_eq!(channel, "telegram");
        assert!(message.contains("Oficina A") && message.contains("Oficina B") && message.contains("Firefox"), "{message}");
        assert!(!message.contains("Camoufox"));
    }
}

#[test]
fn a_failed_search_sends_one_error_alert_to_the_webhook() {
    let sent: Sent = Default::default();
    let (mut notifications, _completed) = live(&sent);
    notifications.observe(NotificationStateInput { failed: true, page: PageKind::Unknown, ..Default::default() });
    notifications.observe(NotificationStateInput { failed: true, page: PageKind::Unknown, ..Default::default() });
    drop(notifications);
    let sent = sent.lock().unwrap();
    assert_eq!(sent.iter().filter(|(channel, _)| channel == "webhook").count(), 1, "{sent:?}");
}

#[test]
fn worker_delivers_off_thread_and_drops_stale_intents() {
    let mut planner = NotificationPlanner::default();
    let telegram = config();
    let state = |document: &str| json!({"run_id":"r","document":document,"mode":"manual","screen":"Выбор офиса","stage":"office","available_offices":["A"]});
    let first = planner.observe("p", "Demo", "Madrid", &state("d1"), "en", &telegram).unwrap();
    let second = planner.observe("p", "Demo", "Madrid", &state("d2"), "en", &telegram).unwrap();
    let (delivered, results): (Sent, Arc<Mutex<Vec<String>>>) = Default::default();
    let threads = Arc::new(Mutex::new(Vec::new()));
    let prepared = prepare_notifications("p", &json!({}), &Vault::new(Locked::default()), None).unwrap();
    let (log, seen, outcome) = (delivered.clone(), threads.clone(), results.clone());
    let worker = NotificationWorker::spawn(
        prepared,
        move |intent, _| {
            log.lock().unwrap().push((intent.channel().into(), intent.message().into()));
            seen.lock().unwrap().push(std::thread::current().id());
            Ok(DeliveryOutcome::TelegramAccepted)
        },
        move |channel, result| outcome.lock().unwrap().push(format!("{channel}:{}", result.is_ok())),
    );
    worker.submit(first);
    worker.submit(second);
    drop(worker);
    assert_eq!(delivered.lock().unwrap().len(), 1, "the stale d1 intent is never delivered");
    assert_ne!(threads.lock().unwrap()[0], std::thread::current().id());
    assert_eq!(results.lock().unwrap().as_slice(), ["telegram:false", "telegram:true"]);
}

#[derive(Default)]
struct Locked {
    values: RefCell<BTreeMap<String, String>>,
    reads: Cell<usize>,
    locked: Cell<bool>,
}
impl Backend for Locked {
    fn get(&self, _: &str, key: &str) -> Result<Option<String>, VaultError> {
        self.reads.set(self.reads.get() + 1);
        if self.locked.get() {
            return Err(VaultError::Locked);
        }
        Ok(self.values.borrow().get(key).cloned())
    }
    fn set(&self, _: &str, key: &str, value: &str) -> Result<(), VaultError> {
        self.values.borrow_mut().insert(key.into(), value.into());
        Ok(())
    }
    fn delete(&self, _: &str, key: &str) -> Result<(), VaultError> {
        self.values.borrow_mut().remove(key);
        Ok(())
    }
}

fn profile_with_notifications(root: &std::path::Path, vault: &Vault<Locked>, enabled: bool) -> String {
    let editor = ceh_desktop::profile_editor(root, None).unwrap();
    let mut draft = editor.draft;
    draft.label = "Fixture".into();
    draft.province = "Barcelona".into();
    draft.fill_mode = "copy".into();
    draft.nationality = "ALBANIA".into();
    draft.phone = "600000000".into();
    draft.email = "fixture@example.test".into();
    let notifications: ceh_desktop::notification_editor::NotificationDraft = serde_json::from_value(json!({
        "telegram":{"enabled":enabled,"events":["booking"],"recipients":[{"id":"123456","label":"Fixture","type":"private","username":""}],"token":"123456:FAKE_TOKEN"},
        "webhook":{"enabled":false,"events":["error"]}
    }))
    .unwrap();
    ceh_desktop::notification_editor::save(root, None, draft, &editor.revision, None, Some(&notifications), vault).unwrap().id
}

#[test]
fn start_preparation_reads_enabled_secrets_and_fails_closed_when_unreadable() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::new(Locked::default());
    let id = profile_with_notifications(root.path(), &vault, true);
    let (_config, prepared) = prepare_for_start(root.path(), &id, &vault).unwrap();
    assert_eq!(prepared.profile(), id);
    vault.backend().locked.set(true);
    assert_eq!(prepare_for_start(root.path(), &id, &vault).err(), Some("notification_secrets_unavailable"));
}

#[test]
fn start_preparation_without_enabled_channels_never_touches_the_vault() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::new(Locked::default());
    let id = profile_with_notifications(root.path(), &vault, false);
    vault.backend().reads.set(0);
    vault.backend().locked.set(true);
    assert!(prepare_for_start(root.path(), &id, &vault).is_ok());
    assert_eq!(vault.backend().reads.get(), 0);
    assert_eq!(NotificationError::Stale, NotificationError::Stale);
}

#[test]
fn a_sign_in_and_a_stop_tell_the_person_the_service_the_method_the_time_and_the_reason() {
    let sent: Sent = Default::default();
    let log = sent.clone();
    let prepared = prepare_notifications("p", &json!({}), &Vault::new(Locked::default()), None).unwrap();
    let (completed, receipt) = mpsc::channel();
    let mut live = LiveSettings {
        config: json!({"telegram":{"enabled":true,"events":["auth","error"],"recipients":[{"id":"1"}],"secret_ref":""}}),
        service: "Toma de huellas / TIE".into(),
        access: "movil".into(),
        timezone: "Europe/Madrid".into(),
        ..settings()
    };
    live.browser = "Chrome".into();
    let mut notifications = LiveNotifications::start(
        live,
        prepared,
        move |intent, _| {
            log.lock().unwrap().push((intent.channel().into(), intent.message().into()));
            Ok(DeliveryOutcome::TelegramAccepted)
        },
        move |_, _| { let _ = completed.send(()); },
    );
    notifications.observe(NotificationStateInput { document: Some("d".into()), page: PageKind::Auth, attention: true, ..Default::default() });
    receipt.recv_timeout(Duration::from_secs(5)).expect("the sign-in message is delivered");
    notifications.observe(NotificationStateInput {
        failed: true,
        reason: Some("browser_action_failed".into()),
        stop_page: Some(PageKind::Expired),
        checks: Some(14),
        ..Default::default()
    });
    receipt.recv_timeout(Duration::from_secs(5)).expect("the stop message is delivered");
    drop(notifications);
    let sent = sent.lock().unwrap();
    assert_eq!(sent.len(), 2, "{sent:?}");
    let (auth, error) = (&sent[0].1, &sent[1].1);
    assert!(auth.contains("Sign-in needed: Cl@ve Móvil") && auth.contains("Procedure: Toma de huellas / TIE · Madrid"), "{auth}");
    assert!(error.contains("Search stopped") && error.contains("Where: the page “Session expired”"), "{error}");
    assert!(error.contains("Reason: the action was not confirmed; the Chrome window was left open."), "{error}");
    let stopped = error.lines().find(|line| line.starts_with("Stopped at ")).expect("the time of the stop");
    let clock = stopped.trim_start_matches("Stopped at ").split(" · ").next().unwrap();
    assert!(clock.len() == 5 && clock.as_bytes()[2] == b':', "{stopped}");
    assert!(stopped.ends_with("checks in this run: 14"), "{stopped}");
}

#[test]
fn the_wait_on_a_page_is_counted_from_its_first_look_and_starts_again_on_another_page_or_after_a_break() {
    let mut watch = HoldWatch::default();
    assert_eq!(watch.update("d1", false, 0), None);
    assert_eq!(watch.update("d1", true, 1_000), Some(0));
    assert_eq!(watch.update("d1", true, 31_000), Some(30));
    assert_eq!(watch.update("d1", true, 62_500), Some(61));
    assert_eq!(watch.update("d2", true, 70_000), Some(0), "another page starts again");
    assert_eq!(watch.update("d2", false, 80_000), None);
    assert_eq!(watch.update("d2", true, 90_000), Some(0), "a break starts again");
}
