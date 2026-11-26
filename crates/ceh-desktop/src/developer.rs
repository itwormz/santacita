//! The developer window (three clicks on the small icon in About): the track recording, the live state of the search,
//! the app's folders, an export of the last session and the environment. Nothing here shows or exports what the person
//! typed or keeps in a profile: the live state holds names, counts and codes only, the export takes the timeline and the
//! report that diagnostics already wrote without values, and the environment holds versions and paths.
use serde_json::{Value, json};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

/// Where the track recording goes when it is turned on from the developer window.
pub fn learning_root(root: &Path) -> PathBuf {
    root.join("data").join("learning")
}

/// Turn the track recording on (into the app's own `learning` folder) or off. A search running now picks it up at its
/// next pass; the next searches start with it.
pub fn set_learning(root: &Path, on: bool) -> Result<Option<PathBuf>, &'static str> {
    if !on {
        crate::learning_recorder::request(None);
        return Ok(None);
    }
    let folder = learning_root(root);
    private_folder(&folder)?;
    crate::learning_recorder::request(Some(folder.clone()));
    Ok(Some(folder))
}

/// The app folders the window can open.
pub fn folder(root: &Path, name: &str) -> Result<PathBuf, &'static str> {
    let path = match name {
        "data" => root.join("data"),
        "diagnostics" => crate::diagnostics_directory(root)?,
        "reports" => root.join("data").join("startup-errors"),
        "learning" => crate::learning_recorder::requested().unwrap_or_else(|| learning_root(root)),
        _ => return Err("developer_folder_unknown"),
    };
    private_folder(&path)?;
    Ok(path)
}

/// What the window polls: the recording, the folders and the live state (the environment is read once, it is slower).
pub fn state(root: &Path) -> Value {
    let learning = crate::learning_recorder::requested();
    json!({
        "learning": {"on": learning.is_some(), "folder": learning.map(|p| p.display().to_string())},
        "folders": {
            "data": root.join("data").display().to_string(),
            "diagnostics": root.join("data").join("diagnostics").display().to_string(),
            "reports": root.join("data").join("startup-errors").display().to_string(),
            "learning": learning_root(root).display().to_string(),
        },
        "live": crate::session_report::live(),
    })
}

/// Versions, the system and the browsers this machine has, with the arguments a Chromium browser is started with.
pub fn environment(root: &Path) -> Value {
    let browsers: Vec<Value> = ceh_browser::discovery::discover_installed()
        .iter()
        .map(|b| json!({"id": b.family.id(), "name": b.name, "version": b.version, "path": b.executable.display().to_string(), "support": b.support.id()}))
        .collect();
    let os = if cfg!(target_os = "macos") {
        ceh_browser::launch::TargetOs::Macos
    } else if cfg!(target_os = "windows") {
        ceh_browser::launch::TargetOs::Windows
    } else {
        ceh_browser::launch::TargetOs::Linux
    };
    let chromium = ceh_browser::chromium::launch_arguments(Path::new("<profile>"), os);
    json!({
        "app": env!("CARGO_PKG_VERSION"),
        "system": std::env::consts::OS,
        "architecture": std::env::consts::ARCH,
        "data": root.display().to_string(),
        "browsers": browsers,
        "chromium_arguments": chromium,
        "learning_variable": std::env::var_os("CEH_LEARNING").is_some(),
    })
}

/// The environment as plain text for the clipboard.
pub fn environment_text(root: &Path) -> String {
    let env = environment(root);
    let mut text = format!(
        "SantaCita {}\nSystem: {} {}\nData: {}\n",
        env["app"].as_str().unwrap_or(""),
        env["system"].as_str().unwrap_or(""),
        env["architecture"].as_str().unwrap_or(""),
        env["data"].as_str().unwrap_or("")
    );
    for browser in env["browsers"].as_array().into_iter().flatten() {
        text.push_str(&format!(
            "Browser: {} {} ({}) {}\n",
            browser["name"].as_str().unwrap_or(""),
            browser["version"].as_str().unwrap_or(""),
            browser["support"].as_str().unwrap_or(""),
            browser["path"].as_str().unwrap_or("")
        ));
    }
    let arguments: Vec<&str> = env["chromium_arguments"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    text.push_str(&format!("Chromium arguments: {}\n", arguments.join(" ")));
    text
}

/// A zip of the newest search session (its timeline), the newest saved report (scrubbed as for the clipboard) and the
/// environment, written to `into` (the person's Downloads). Returns the file.
pub fn export_last_session(root: &Path, home: &str, into: &Path, stamp: &str) -> Result<PathBuf, &'static str> {
    let sessions = root.join("data").join("diagnostics").join("sessions");
    let newest = std::fs::read_dir(&sessions)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("timeline.jsonl").is_file())
        .max_by_key(|entry| entry.path().join("timeline.jsonl").metadata().and_then(|m| m.modified()).ok());
    let report = crate::startup_report::latest_id(root, home).and_then(|id| crate::startup_report::clipboard_text(root, home, &id).ok());
    if newest.is_none() && report.is_none() {
        return Err("developer_nothing_to_export");
    }
    std::fs::create_dir_all(into).map_err(|_| "developer_export_failed")?;
    let file = into.join(format!("SantaCita-debug-{stamp}.zip"));
    let out = std::fs::File::create(&file).map_err(|_| "developer_export_failed")?;
    let mut zip = zip::ZipWriter::new(out);
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut add = |name: &str, bytes: &[u8]| -> Result<(), &'static str> {
        zip.start_file(name, options).map_err(|_| "developer_export_failed")?;
        zip.write_all(bytes).map_err(|_| "developer_export_failed")
    };
    if let Some(entry) = newest {
        let timeline = std::fs::read(entry.path().join("timeline.jsonl")).map_err(|_| "developer_export_failed")?;
        add(&format!("session-{}/timeline.jsonl", entry.file_name().to_string_lossy()), &timeline)?;
    }
    if let Some(report) = report {
        add("report.txt", report.as_bytes())?;
    }
    add("environment.txt", environment_text(root).as_bytes())?;
    zip.finish().map_err(|_| "developer_export_failed")?;
    Ok(file)
}

/// What the site check saw: the start page opened, the browser could not verify the site's certificate, the site did
/// not open, the site blocked the browser, or nothing recognisable within the time.
pub fn site_check_result(kind: ceh_core::model::PageKind, variant: &str) -> &'static str {
    use ceh_core::model::PageKind;
    match kind {
        PageKind::Unreachable if ceh_core::recovery::is_certificate_error(variant) => "certificate_error",
        PageKind::Unreachable => "unreachable",
        PageKind::Blocked | PageKind::RateLimited => "blocked",
        PageKind::Loading | PageKind::Unknown | PageKind::AuthWait => "pending",
        _ => "opened",
    }
}

/// Open the site's start page in a throwaway profile of `browser` (`firefox`, `chrome`, `brave`, `chromium`, `edge`)
/// and report what happened, then close it. Nothing is clicked: it never goes past the start page.
pub fn site_check(browser: &str) -> Value {
    use ceh_browser::firefox::{FirefoxInstallation, OwnedFirefox, ProfileKind, ProfileLease};
    let started = std::time::Instant::now();
    let Ok(runtime) = tempfile::tempdir() else { return json!({"result": "start_failed"}) };
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let kind = match browser {
        "firefox" => None,
        "chrome" => Some(ProfileKind::Chrome),
        "brave" => Some(ProfileKind::Brave),
        "chromium" => Some(ProfileKind::Chromium),
        "edge" if cfg!(target_os = "windows") => Some(ProfileKind::Edge),
        _ => return json!({"result": "browser_unsupported"}),
    };
    let launched = match kind {
        None => FirefoxInstallation::discover().ok().and_then(|installation| {
            let lease = ProfileLease::temporary(runtime.path()).ok()?;
            OwnedFirefox::start(&installation, lease, false, &cancelled).ok()
        }),
        Some(kind) => ceh_browser::chromium::ChromiumInstallation::discover(kind).ok().and_then(|installation| {
            let lease = ProfileLease::temporary_kind(runtime.path(), kind).ok()?;
            OwnedFirefox::start_chromium(&installation, lease, &cancelled).ok()
        }),
    };
    let Some(mut owned) = launched else { return json!({"result": "start_failed"}) };
    let context = owned.context().to_owned();
    let _ = owned.connection().call(
        "browsingContext.navigate",
        json!({"context": context, "url": ceh_browser::recovery_navigation::ENTRY_URL, "wait": "none"}),
    );
    let mut result = json!({"result": "timeout"});
    if let Ok(mut observer) = ceh_browser::observer::PageObserver::new(&context) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(700));
            let Ok(page) = observer.observe(owned.connection(), "manual") else { continue };
            let outcome = site_check_result(page.observation.kind, &page.observation.variant);
            if outcome != "pending" {
                result = json!({"result": outcome, "page": format!("{:?}", page.observation.kind), "code": page.observation.variant});
                break;
            }
        }
    }
    let _ = owned.stop();
    result["browser"] = json!(browser);
    result["seconds"] = json!(started.elapsed().as_secs());
    result
}

fn private_folder(path: &Path) -> Result<(), &'static str> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(|_| "developer_folder_unavailable")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_recording_is_turned_on_into_the_app_folder_and_off_again() {
        let root = tempfile::tempdir().unwrap();
        let on = set_learning(root.path(), true).unwrap().unwrap();
        assert_eq!(on, learning_root(root.path()));
        assert!(on.is_dir());
        assert_eq!(crate::learning_recorder::requested(), Some(on));
        assert_eq!(set_learning(root.path(), false).unwrap(), None);
        assert_eq!(crate::learning_recorder::requested(), None);
        assert_eq!(state(root.path())["learning"]["on"], false);
    }
    #[test]
    fn the_site_check_tells_a_certificate_error_from_a_site_that_does_not_open() {
        use ceh_core::model::PageKind;
        assert_eq!(site_check_result(PageKind::Unreachable, "ERR_CERT_AUTHORITY_INVALID"), "certificate_error");
        assert_eq!(site_check_result(PageKind::Unreachable, "dnsNotFound"), "unreachable");
        assert_eq!(site_check_result(PageKind::Blocked, ""), "blocked");
        assert_eq!(site_check_result(PageKind::Loading, ""), "pending");
        assert_eq!(site_check_result(PageKind::Province, ""), "opened");
    }
    #[test]
    fn only_the_known_folders_are_opened() {
        let root = tempfile::tempdir().unwrap();
        for name in ["data", "diagnostics", "reports", "learning"] {
            assert!(folder(root.path(), name).unwrap().starts_with(root.path()), "{name}");
        }
        assert_eq!(folder(root.path(), "../etc"), Err("developer_folder_unknown"));
    }
    #[test]
    fn the_export_holds_the_newest_timeline_and_the_environment() {
        let root = tempfile::tempdir().unwrap();
        let sessions = root.path().join("data/diagnostics/sessions");
        for (id, text) in [("old", "{\"old\":1}\n"), ("new", "{\"new\":1}\n")] {
            std::fs::create_dir_all(sessions.join(id)).unwrap();
            std::fs::write(sessions.join(id).join("timeline.jsonl"), text).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let out = tempfile::tempdir().unwrap();
        let file = export_last_session(root.path(), "/nonexistent-home", out.path(), "t1").unwrap();
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&file).unwrap()).unwrap();
        let names: Vec<String> = (0..zip.len()).map(|i| zip.by_index(i).unwrap().name().unwrap().to_string()).collect();
        assert!(names.contains(&"session-new/timeline.jsonl".to_owned()), "{names:?}");
        assert!(names.contains(&"environment.txt".to_owned()), "{names:?}");
        assert!(!names.iter().any(|n| n.contains("old")), "{names:?}");
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(export_last_session(empty.path(), "/nonexistent-home", out.path(), "t2"), Err("developer_nothing_to_export"));
    }
}
