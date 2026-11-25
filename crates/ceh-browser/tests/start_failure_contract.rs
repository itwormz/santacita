//! Where a browser start stopped: each place has its own code, so a report says what happened on the person's computer.
#![cfg(unix)]
use ceh_browser::firefox::{FirefoxError, FirefoxInstallation, OwnedFirefox, ProfileLease, take_start_detail};
use std::{os::unix::fs::PermissionsExt, path::Path, sync::atomic::AtomicBool};

/// A Firefox installation whose program is `script`, run with `sh`.
fn start_with_script(script: &str, executable: bool) -> Result<OwnedFirefox, FirefoxError> {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("firefox");
    std::fs::write(&bin, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 })).unwrap();
    std::fs::write(root.path().join("application.ini"), "[App]\nName=Firefox\nVersion=158.0.1\n").unwrap();
    let installation = FirefoxInstallation::from_executable(&bin).unwrap();
    let profile = ProfileLease::temporary(Path::new(root.path())).unwrap();
    OwnedFirefox::start(&installation, profile, true, &AtomicBool::new(false))
}

#[test]
fn a_program_the_system_will_not_run_is_a_failed_launch() {
    assert!(matches!(start_with_script("exit 0", false), Err(FirefoxError::LaunchFailed)));
}
#[test]
fn a_browser_that_ends_at_once_is_an_early_exit() {
    assert!(matches!(start_with_script("exit 3", true), Err(FirefoxError::ExitedEarly)));
}
#[test]
fn a_browser_that_closes_its_output_without_an_address_has_no_endpoint() {
    assert!(matches!(start_with_script("exec 2>&-\nsleep 5", true), Err(FirefoxError::NoEndpoint)));
}
#[test]
fn an_address_nobody_answers_on_is_a_failed_connection() {
    let announced = "echo 'WebDriver BiDi listening on ws://127.0.0.1:1' >&2\nsleep 5";
    assert!(matches!(start_with_script(announced, true), Err(FirefoxError::ConnectFailed)));
}
#[test]
fn every_start_failure_has_its_own_fixed_code() {
    let codes: Vec<_> = [
        FirefoxError::Unavailable, FirefoxError::LaunchFailed, FirefoxError::ExitedEarly, FirefoxError::NoEndpoint,
        FirefoxError::ConnectFailed, FirefoxError::SessionFailed, FirefoxError::StartupTimeout,
    ]
    .iter()
    .map(FirefoxError::code)
    .collect();
    assert_eq!(codes, ["unavailable", "launch_failed", "exited_early", "no_endpoint", "connect_failed", "session_failed", "startup_timeout"]);
}
#[test]
fn a_browser_that_ended_at_once_leaves_its_exit_status_its_version_and_the_time_it_took() {
    let _ = take_start_detail();
    assert!(matches!(start_with_script("exit 3", true), Err(FirefoxError::ExitedEarly)));
    let detail = take_start_detail().expect("a failed start leaves its detail");
    assert_eq!((detail.exit_code, detail.browser_version.as_str()), (Some(3), "158.0.1"));
    assert!(detail.elapsed_ms < 20_000);
    assert_eq!(take_start_detail(), None, "taken once");
}
#[test]
fn the_detail_of_one_start_does_not_stay_for_the_next() {
    assert!(matches!(start_with_script("exit 3", true), Err(FirefoxError::ExitedEarly)));
    assert!(matches!(start_with_script("exit 0", false), Err(FirefoxError::LaunchFailed)));
    let detail = take_start_detail().unwrap();
    assert_eq!(detail.exit_code, None, "the program never ran, so there is no status");
}
#[test]
fn the_last_three_lines_the_browser_wrote_before_it_ended_are_kept() {
    let _ = take_start_detail();
    let script = "echo one >&2\necho two >&2\necho 'cannot lock the profile: busy' >&2\necho 'second line\tof text' >&2\nexit 1";
    assert!(matches!(start_with_script(script, true), Err(FirefoxError::ExitedEarly)));
    let detail = take_start_detail().unwrap();
    assert_eq!(detail.stderr_tail, ["two", "cannot lock the profile: busy", "second lineof text"]);
}
#[test]
fn a_very_long_line_is_cut_and_an_empty_one_is_not_kept() {
    let _ = take_start_detail();
    let script = format!("echo '' >&2\necho {} >&2\nexit 1", "x".repeat(500));
    assert!(matches!(start_with_script(&script, true), Err(FirefoxError::ExitedEarly)));
    let detail = take_start_detail().unwrap();
    // The shell that runs the stand-in may add a line of its own; what matters is the last one and that none is empty.
    assert_eq!(detail.stderr_tail.last().unwrap(), &"x".repeat(160));
    assert!(detail.stderr_tail.iter().all(|line| !line.is_empty()));
}
#[test]
fn the_last_words_of_a_browser_that_wrote_no_final_line_break_are_kept() {
    let _ = take_start_detail();
    let script = "echo first >&2\nprintf 'fatal: no display' >&2\nexit 1";
    assert!(matches!(start_with_script(script, true), Err(FirefoxError::ExitedEarly)));
    let detail = take_start_detail().unwrap();
    let last = detail.stderr_tail.last().map(String::as_str);
    assert_eq!(last, Some("fatal: no display"), "{:?}", detail.stderr_tail);
}
#[test]
fn a_chromium_that_ended_at_once_leaves_what_it_wrote_like_firefox_does() {
    use ceh_browser::{chromium::ChromiumInstallation, firefox::ProfileKind};
    let _ = take_start_detail();
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("chromium");
    std::fs::write(
        &bin,
        "#!/bin/sh\ncase \"$1\" in\n--version) echo 'Chromium 120.0.6099.109';;\n*) echo 'GPU process crashed' >&2; printf 'no display' >&2; exit 5;;\nesac\n",
    )
    .unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    let installation = ChromiumInstallation::from_executable(ProfileKind::Chromium, &bin).unwrap();
    let profile = ProfileLease::temporary(Path::new(root.path())).unwrap();
    let result = OwnedFirefox::start_chromium(&installation, profile, &AtomicBool::new(false));
    assert!(matches!(result, Err(FirefoxError::ExitedEarly)));
    let detail = take_start_detail().unwrap();
    assert_eq!(detail.exit_code, Some(5));
    let tail: Vec<&str> = detail.stderr_tail.iter().map(String::as_str).collect();
    assert!(tail.ends_with(&["GPU process crashed", "no display"]), "{tail:?}");
}
