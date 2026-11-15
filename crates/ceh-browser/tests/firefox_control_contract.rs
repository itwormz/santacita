#![cfg(feature = "bidi")]
use ceh_browser::firefox::startup_preferences;
use ceh_browser::firefox_control::{Fallback, WebdriverMode, WEBDRIVER_MASK_V2, parse_port_report, restart_script};
use ceh_browser::launch::TargetOs;

#[test]
fn the_remote_control_preferences_are_set_and_automation_defaults_are_not_applied() {
    let prefs = startup_preferences(TargetOs::Linux);
    assert_eq!(prefs["remote.experimental.dynamicstart.enabled"], true);
    assert_eq!(prefs["remote.experimental.dynamicstart.prompt.enabled"], false);
    assert_eq!(prefs["remote.prefs.recommended"], false);
}

#[test]
fn a_restart_report_gives_the_port_only_when_automation_mode_is_off() {
    assert_eq!(parse_port_report("51234 false"), Ok(51234));
    assert_eq!(parse_port_report("51234 true"), Err(Fallback::RestartFailed));
    assert_eq!(parse_port_report("ERR TypeError: x"), Err(Fallback::RestartFailed));
    assert_eq!(parse_port_report("0 false"), Err(Fallback::RestartFailed));
    assert_eq!(parse_port_report("garbage"), Err(Fallback::RestartFailed));
    assert_eq!(parse_port_report("70000 false"), Err(Fallback::RestartFailed));
}

#[test]
fn the_restart_script_embeds_only_a_json_encoded_path() {
    let script = restart_script("C:\\Users\\a\"b\\ceh-remote-port");
    assert!(script.contains(r#""C:\\Users\\a\"b\\ceh-remote-port""#), "{script}");
    assert!(script.contains("startAtRuntime({ isBrowserAutomation: false })"));
    assert!(script.contains("typeof RemoteAgent.startAtRuntime"));
    assert!(script.contains("typeof RemoteAgent.stopAtRuntime"), "a Firefox with only one of the two is unsupported before anything is stopped");
}

#[test]
fn the_mask_keeps_the_native_source_of_the_getter_and_of_to_string() {
    assert!(WEBDRIVER_MASK_V2.contains("WeakMap"));
    assert!(WEBDRIVER_MASK_V2.contains("Function.prototype"));
}

#[test]
fn modes_have_stable_diagnostic_names() {
    assert_eq!(WebdriverMode::RemoteControl.name(), "remote_control");
    assert_eq!(WebdriverMode::Mask.name(), "mask");
    assert_eq!(WebdriverMode::NotNeeded.name(), "not_needed");
    assert_eq!(Fallback::NoSystemAccess.name(), "no_system_access");
    assert_eq!(Fallback::Unsupported.name(), "unsupported");
    assert_eq!(Fallback::RestartFailed.name(), "restart_failed");
    assert_eq!(Fallback::Timeout.name(), "timeout");
}

#[test]
fn only_a_rejected_command_means_the_browser_has_no_system_access() {
    use ceh_browser::bidi::ProtocolError;
    use ceh_browser::firefox_control::fallback_for_command_error;
    // The browser answered "no": the first connection still works and the mask is installed on it.
    assert_eq!(fallback_for_command_error(ProtocolError::Rejected), Fallback::NoSystemAccess);
    // A timeout or a broken connection poisons the connection: Firefox is restarted with the mask instead.
    for broken in [ProtocolError::Unavailable, ProtocolError::InvalidData, ProtocolError::PendingCommand, ProtocolError::EventOverflow, ProtocolError::InvalidEndpoint] {
        assert_eq!(fallback_for_command_error(broken), Fallback::RestartFailed, "{broken:?}");
    }
}
