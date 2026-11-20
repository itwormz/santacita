//! Development only, never touches the real site: opens addresses that cannot be reached (a closed local port, a
//! name that does not exist, a black-hole address) in a fresh temporary profile and prints what the page looks like
//! to the bot: the address, the origin, the title, the start of the text and the page kind it is recognized as.
//! Usage: cargo run -p ceh-cli --example unreachable_probe --release -- firefox|chrome
use ceh_browser::{
    chromium::ChromiumInstallation,
    firefox::{FirefoxInstallation, OwnedFirefox, ProfileKind, ProfileLease},
    observer::PageObserver,
};
use std::{path::Path, sync::atomic::AtomicBool, time::{Duration, Instant}};

const TARGETS: [(&str, &str, u64); 3] = [
    ("connection refused", "http://127.0.0.1:9/icpplus/index.html", 6),
    ("name not found", "https://santacita-probe.invalid/icpplus/index.html", 8),
    ("black hole (timeout)", "http://10.255.255.1/icpplus/index.html", 45),
];

fn main() {
    let which = std::env::args().nth(1).unwrap_or_else(|| "firefox".into());
    let runtime = Path::new(".local/builds/work/unreachable-probe");
    std::fs::create_dir_all(runtime).expect("work folder");
    let runtime = runtime.canonicalize().expect("work folder path");
    let lease = ProfileLease::temporary(&runtime).expect("temporary profile");
    let cancelled = AtomicBool::new(false);
    let mut browser = if which == "chrome" {
        let installation = ChromiumInstallation::discover(ProfileKind::Chrome).expect("Chrome");
        OwnedFirefox::start_chromium(&installation, lease, &cancelled).expect("start Chrome")
    } else {
        let installation = FirefoxInstallation::discover().expect("Firefox");
        OwnedFirefox::start(&installation, lease, false, &cancelled).expect("start Firefox")
    };
    let context = browser.context().to_owned();
    for (name, url, wait_seconds) in TARGETS {
        let started = Instant::now();
        let navigated = browser
            .connection()
            .call("browsingContext.navigate", serde_json::json!({"context": context, "url": url, "wait": "none"}));
        println!("== {which}: {name} ({url}) navigate: {}", if navigated.is_ok() { "ok".into() } else { format!("{navigated:?}") });
        let mut observer = PageObserver::new(&context).expect("observer");
        let mut last = String::new();
        while started.elapsed() < Duration::from_secs(wait_seconds) {
            std::thread::sleep(Duration::from_millis(700));
            let facts = browser.connection().call(
                "script.evaluate",
                serde_json::json!({"expression": "JSON.stringify({href:location.href,origin:location.origin,title:document.title,ready:document.readyState,uri:document.documentURI,text:((document.body&&document.body.innerText)||'').replace(/\\s+/g,' ').slice(0,260)})", "target": {"context": context}, "awaitPromise": false}),
            );
            let line = match facts {
                Ok(value) => value["result"]["value"].as_str().unwrap_or("?").to_owned(),
                Err(error) => format!("evaluate failed: {error:?}"),
            };
            let kind = match observer.observe(browser.connection(), "manual") {
                Ok(page) => format!("kind={:?} recognized={:?} official={} code={:?}", page.observation.kind, page.recognition.kind, page.page.snapshot.official, page.recognition.variant),
                Err(error) => format!("observe failed: {error:?}"),
            };
            let shown = format!("{kind} | {line}");
            if shown != last {
                println!("  {:5.1}s {shown}", started.elapsed().as_secs_f64());
                last = shown;
            }
        }
    }
    let _ = browser.stop();
}
