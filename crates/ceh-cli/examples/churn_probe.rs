//! Development only: starts a browser, loads a page and closes it again, many times in one process, the way a search
//! that is blocked again and again restarts its session. Run it with the system's memory checks on to see a
//! corrupted or wrongly freed block close to its cause, for example
//! `MallocScribble=1 MallocPreScribble=1 MallocGuardEdges=1 MallocCheckHeapStart=2000 MallocCheckHeapEach=2000`.
//! Usage: cargo run -p ceh-cli --example churn_probe --release -- chrome|brave|firefox [cycles]
use ceh_browser::{
    chromium::ChromiumInstallation,
    firefox::{FirefoxInstallation, OwnedFirefox, ProfileKind, ProfileLease},
};
use std::{path::Path, sync::atomic::AtomicBool, time::Duration};

fn main() {
    let mut args = std::env::args().skip(1);
    let engine = args.next().unwrap_or_else(|| "chrome".into());
    let cycles: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(20);
    let runtime = Path::new(".local/builds/work/churn-runtime");
    std::fs::create_dir_all(runtime).expect("work folder");
    let runtime = runtime.canonicalize().expect("work folder path");
    let cancelled = AtomicBool::new(false);
    for cycle in 1..=cycles {
        let lease = ProfileLease::temporary(&runtime).expect("temporary profile");
        let mut browser = match engine.as_str() {
            "firefox" => {
                let installation = FirefoxInstallation::discover().expect("Firefox");
                OwnedFirefox::start(&installation, lease, false, &cancelled).expect("start Firefox")
            }
            other => {
                let kind = if other == "brave" { ProfileKind::Brave } else { ProfileKind::Chrome };
                let installation = ChromiumInstallation::discover(kind).expect("Chromium browser");
                OwnedFirefox::start_chromium(&installation, lease, &cancelled).expect("start browser")
            }
        };
        let samples = [0.7, (cycle % 10) as f64 / 10.0, ((cycle * 3) % 10) as f64 / 10.0, 0.5];
        let _ = browser.arrange_window(samples);
        let context = browser.context().to_owned();
        let navigated = browser.connection().call(
            "browsingContext.navigate",
            serde_json::json!({"context": context, "url": "https://example.com/", "wait": "none"}),
        );
        std::thread::sleep(Duration::from_millis(1500));
        for _ in 0..20 {
            let _ = browser.connection().call(
                "script.evaluate",
                serde_json::json!({"expression": "JSON.stringify({t: document.title, n: document.querySelectorAll('*').length, u: location.href})", "target": {"context": context}, "awaitPromise": false}),
            );
        }
        let stopped = browser.stop();
        println!("cycle {cycle}: navigate {} stop {}", if navigated.is_ok() { "ok" } else { "failed" }, if stopped.is_ok() { "ok" } else { "failed" });
        drop(browser);
    }
    println!("done: {cycles} cycles");
}
