//! The Telegram messages about a sign-in and a stopped search say what is going on and what to do.
//! Office and booking messages are not touched, and nothing personal can get into any of them.
use ceh_services::notifications::{NotificationPlanner, telegram_message, with_browser_name};
use serde_json::{Value, json};

fn config() -> Value {
    json!({"telegram":{"enabled":true,"events":["auth","office","booking","error"],"recipients":[{"id":"1"}],"secret_ref":"a"}})
}
fn telegram(language: &str, state: Value) -> String {
    let mut planner = NotificationPlanner::default();
    let intents = planner.observe("profile", "Barcelona TIE", "Barcelona", &state, language, &config()).unwrap();
    intents.iter().find(|intent| intent.channel() == "telegram").expect("a Telegram message").message().to_owned()
}
fn auth_state(access: &str) -> Value {
    json!({"run_id":"r","document":"d","mode":"manual","screen":"Авторизация","browser":"Chrome","service":"Toma de huellas / TIE","access":access})
}
fn error_state(reason: &str) -> Value {
    json!({"run_id":"r","failed":true,"browser":"Chrome","service":"Toma de huellas / TIE","reason":reason,"stop_page":"expired","at":"16:39","checks":14})
}

#[test]
fn a_sign_in_message_names_the_method_and_what_to_do() {
    let first_message = telegram("ru", auth_state("certificate"));
    for line in ["🔐 Нужен вход: сертификат", "Профиль: Barcelona TIE", "Услуга: Toma de huellas / TIE · Barcelona", "Что сделать: в окне Chrome выберите сертификат и подтвердите.", "Поиск продолжится сам после входа."] {
        assert!(first_message.contains(line), "missing {line:?} in:\n{first_message}");
    }
    assert!(telegram("ru", auth_state("movil")).contains("Нужен вход: Cl@ve Móvil") && telegram("ru", auth_state("movil")).contains("подтвердите вход в приложении Cl@ve на телефоне"));
    assert!(telegram("ru", auth_state("permanente")).contains("Нужен вход: Cl@ve Permanente") && telegram("ru", auth_state("permanente")).contains("введите PIN Cl@ve в окне Chrome"));
    // The method is not known: still says what to do.
    let unknown = telegram("ru", json!({"run_id":"r","document":"d","mode":"manual","screen":"Авторизация","browser":"Chrome"}));
    assert!(unknown.contains("Нужен вход") && unknown.contains("в окне Chrome завершите вход") && !unknown.contains("Услуга:"), "{unknown}");
}

#[test]
fn a_stopped_search_says_where_why_and_what_to_do() {
    let message = telegram("ru", error_state("browser_action_failed"));
    for line in ["⚠️ Поиск остановился", "Услуга: Toma de huellas / TIE · Barcelona", "Остановка в 16:39 · проверок за этот запуск: 14", "Где: страница «Сессия истекла»", "Причина: действие не подтверждено, окно Chrome оставлено открытым.", "Что сделать: откройте SantaCita и нажмите «Запустить»."] {
        assert!(message.contains(line), "missing {line:?} in:\n{message}");
    }
    assert!(!message.contains("Camoufox") && !message.contains("Firefox"), "the chosen browser is named: {message}");
    // Another known reason, an unknown code, and a hostile one.
    assert!(telegram("ru", error_state("browser_connection_lost")).contains("Причина: связь с Chrome потеряна"));
    assert!(telegram("ru", error_state("some_new_code")).contains("Причина: неизвестная ошибка (some_new_code)"));
    let hostile = telegram("ru", error_state("x\nSYNTHETIC-NIE Y1234567X /Users/Fictional"));
    assert!(hostile.contains("Причина: неизвестная ошибка") && !hostile.contains("Y1234567X") && !hostile.contains("Fictional"), "{hostile}");
    // What is not known is left out, not shown as an empty value.
    let bare = telegram("ru", json!({"run_id":"r","failed":true,"browser":"Chrome"}));
    assert!(bare.contains("Поиск остановился") && bare.contains("Что сделать") && !bare.contains("Где:") && !bare.contains("Причина:") && !bare.contains("Остановка в") && !bare.contains("None") && !bare.contains("null"), "{bare}");
}

#[test]
fn the_new_messages_are_translated() {
    for language in ["en", "es"] {
        for message in [telegram(language, auth_state("movil")), telegram(language, auth_state("certificate")), telegram(language, error_state("browser_action_failed")), telegram(language, error_state("zzz"))] {
            assert!(!message.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c)), "{language}: untranslated text in:\n{message}");
        }
    }
    assert!(telegram("en", error_state("browser_action_failed")).contains("Search stopped"));
    assert!(telegram("es", auth_state("certificate")).contains("Se necesita iniciar sesión"));
}

#[test]
fn office_and_booking_messages_are_what_they_were() {
    let office = json!({"run_id":"r","document":"d","mode":"manual","screen":"Выбор офиса","stage":"office","available_offices":["Oficina A","Oficina B"],"browser":"Chrome","service":"Toma de huellas / TIE","access":"movil"});
    let expected_office = {
        let event = ceh_services::notifications::make_event("office", "Barcelona TIE", "Barcelona", "ru").unwrap();
        with_browser_name(&telegram_message(&event, "ru", &Value::Null, &json!(["Oficina A", "Oficina B"])), "Chrome")
    };
    assert_eq!(telegram("ru", office), expected_office);
    let details = json!({"office":"Oficina A","offers":[{"date":"2026-12-01","time":"10:00"}]});
    let booking = json!({"run_id":"r","booking_handoff":true,"mode":"manual","browser":"Chrome","booking_details":details,"service":"Toma de huellas / TIE"});
    let expected_booking = {
        let event = ceh_services::notifications::make_event("booking", "Barcelona TIE", "Barcelona", "ru").unwrap();
        with_browser_name(&telegram_message(&event, "ru", &details, &Value::Null), "Chrome")
    };
    assert_eq!(telegram("ru", booking), expected_booking);
}

#[test]
fn nothing_else_from_the_state_reaches_a_message() {
    let mut state = error_state("browser_action_failed");
    state["document_number"] = json!("Y1234567X");
    state["applicant_name"] = json!("SYNTHETIC NAME");
    state["phone"] = json!("600000000");
    let message = telegram("ru", state);
    for planted in ["Y1234567X", "SYNTHETIC NAME", "600000000"] {
        assert!(!message.contains(planted), "{planted} reached the message:\n{message}");
    }
}

#[test]
fn the_webhook_gets_the_same_facts_as_extra_fields_and_office_events_stay_as_they_were() {
    let config = json!({"webhook":{"enabled":true,"events":["auth","office","error"],"secret_ref":"b"}});
    let mut planner = NotificationPlanner::default();
    let intents = planner.observe("profile", "Barcelona TIE", "Barcelona", &error_state("browser_action_failed"), "en", &config).unwrap();
    let event = intents[0].event();
    assert_eq!(event["event"], "error");
    assert_eq!(event["reason_code"], "browser_action_failed");
    assert_eq!(event["service"], "Toma de huellas / TIE");
    assert_eq!(event["stop_page"], "expired");
    assert_eq!(event["checks"], 14);
    assert!(event["message"].as_str().unwrap().contains("Search finished with an error") || event["message"].as_str().unwrap().contains("error"), "the plain message is kept: {}", event["message"]);
    let mut planner = NotificationPlanner::default();
    let intents = planner.observe("profile", "Barcelona TIE", "Barcelona", &auth_state("movil"), "en", &config).unwrap();
    assert_eq!(intents[0].event()["access"], "movil");
    // Office events carry nothing new.
    let office = json!({"run_id":"r","document":"d","mode":"manual","screen":"Выбор офиса","stage":"office","available_offices":["A"],"service":"Toma de huellas / TIE"});
    let mut planner = NotificationPlanner::default();
    let intents = planner.observe("profile", "Barcelona TIE", "Barcelona", &office, "en", &config).unwrap();
    let event = intents[0].event().as_object().unwrap();
    assert!(!event.contains_key("service") && !event.contains_key("reason_code") && !event.contains_key("access"), "{event:?}");
    // A hostile value never gets in.
    let mut state = error_state("browser_action_failed");
    state["service"] = json!("bad\nservice");
    state["reason"] = json!("Y1234567X");
    let mut planner = NotificationPlanner::default();
    let intents = planner.observe("profile", "Barcelona TIE", "Barcelona", &state, "en", &config).unwrap();
    let event = intents[0].event().as_object().unwrap();
    assert!(!event.contains_key("service"), "{event:?}");
    assert!(!event.contains_key("reason_code"), "a reason that is not a plain code is left out: {event:?}");
}

fn stuck_state(document: &str, seconds: u64, reason: &str) -> Value {
    json!({"run_id":"r","document":document,"mode":"manual","screen":"","browser":"Chrome","service":"Toma de huellas / TIE","stuck_seconds":seconds,"hold_reason":reason})
}
fn stuck_config() -> Value {
    // No event of its own: it goes to the channels that ask for a stopped search.
    json!({"telegram":{"enabled":true,"events":["error"],"recipients":[{"id":"1"}],"secret_ref":"a"},"webhook":{"enabled":true,"events":["error"],"secret_ref":"b"}})
}

#[test]
fn a_bot_that_waits_on_a_page_it_does_not_know_asks_for_help_after_a_minute_and_only_once_for_the_page() {
    let mut planner = NotificationPlanner::default();
    let ask = |planner: &mut NotificationPlanner, state: Value| planner.observe("profile", "Barcelona TIE", "Barcelona", &state, "ru", &stuck_config()).unwrap();
    assert!(ask(&mut planner, stuck_state("d1", 5, "no_handler_for_page")).is_empty(), "a short stop is not worth a message");
    assert!(ask(&mut planner, stuck_state("d1", 59, "no_handler_for_page")).is_empty());
    let sent = ask(&mut planner, stuck_state("d1", 60, "no_handler_for_page"));
    assert_eq!(sent.iter().map(|i| i.channel()).collect::<Vec<_>>(), ["telegram", "webhook"]);
    assert!(ask(&mut planner, stuck_state("d1", 600, "no_handler_for_page")).is_empty(), "the same page asks once");
    assert_eq!(ask(&mut planner, stuck_state("d2", 61, "unknown_page_persisted")).len(), 2, "another page asks again");
    let webhook = sent.iter().find(|i| i.channel() == "webhook").unwrap().event();
    assert_eq!((webhook["event"].as_str(), webhook["reason_code"].as_str(), webhook["service"].as_str()), (Some("stuck"), Some("no_handler_for_page"), Some("Toma de huellas / TIE")));
}

#[test]
fn the_help_message_says_what_the_bot_met_what_to_do_and_that_it_goes_on_by_itself() {
    let text = |reason: &str| {
        let mut planner = NotificationPlanner::default();
        let sent = planner.observe("profile", "Barcelona TIE", "Barcelona", &stuck_state("d", 90, reason), "ru", &stuck_config()).unwrap();
        sent.iter().find(|i| i.channel() == "telegram").unwrap().message().to_owned()
    };
    let unknown = text("no_handler_for_page");
    for line in ["✋ Нужна ваша помощь: страница не распознана", "Профиль: Barcelona TIE", "Услуга: Toma de huellas / TIE · Barcelona", "Бот остановился: он не узнаёт открытую страницу сайта.", "Что сделать: в окне Chrome сами выполните шаг на этой странице (нажмите нужную кнопку или вернитесь на страницу поиска).", "Когда страница станет боту знакомой, он сам продолжит поиск, запускать ничего не нужно."] {
        assert!(unknown.contains(line), "missing {line:?} in:\n{unknown}");
    }
    assert_eq!(text("unknown_page_persisted"), unknown);
    let pressed = text("dispatched_page_unchanged");
    assert!(pressed.contains("✋ Нужна ваша помощь: бот остановился") && pressed.contains("Бот нажал кнопку, но страница не изменилась.") && pressed.contains("Что сделать: нажмите её сами в окне Chrome."), "{pressed}");
    assert_eq!(text("action_not_confirmed"), pressed);
    let missing = text("office_count_zero");
    assert!(missing.contains("На странице нет того, что ждёт бот.") && missing.contains("Что сделать: вернитесь на страницу поиска в окне Chrome."), "{missing}");
    assert_eq!(text("a_reason_nobody_knows"), missing, "an unknown reason gets the general words");
    for message in [&unknown, &pressed, &missing] {
        assert!(message.contains("запускать ничего не нужно") && !message.contains("Camoufox"), "{message}");
    }
}

#[test]
fn the_help_message_follows_the_stopped_search_setting_and_is_translated() {
    // It is not an event of its own: the default events are the four they were, and it goes where "error" goes.
    assert_eq!(ceh_services::notifications::defaults()["telegram"]["events"], json!(["auth", "office", "booking", "error"]));
    let without_stop = json!({"telegram":{"enabled":true,"events":["auth","office","booking"],"recipients":[{"id":"1"}],"secret_ref":"a"}});
    let mut planner = NotificationPlanner::default();
    assert!(planner.observe("profile", "Demo", "Barcelona", &stuck_state("d", 120, "no_handler_for_page"), "ru", &without_stop).unwrap().is_empty(), "a channel that does not ask for stops gets nothing");
    for language in ["en", "es"] {
        for reason in ["no_handler_for_page", "dispatched_page_unchanged", "office_count_zero"] {
            let mut planner = NotificationPlanner::default();
            let sent = planner.observe("profile", "Demo", "Barcelona", &stuck_state("d", 90, reason), language, &stuck_config()).unwrap();
            let message = sent.iter().find(|i| i.channel() == "telegram").unwrap().message();
            assert!(!message.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c)), "{language} {reason}: untranslated text in:\n{message}");
        }
    }
}
