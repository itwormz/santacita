//! Development only, never touches the real site: opens public bot-detection test pages with the browser the way
//! a search starts it (fresh temporary profile, same launch arguments) and saves what each page says about the
//! browser (text and a screenshot) so changes to the launch can be compared.
//! Usage: cargo run -p ceh-cli --example detect_probe --release -- chrome|brave|firefox [site ...]
//! Output: `.local/builds/work/detect/<engine>/<site>.txt` and `.png.b64`; the text is also summarised on stdout.
use ceh_browser::{
    chromium::ChromiumInstallation,
    firefox::{FirefoxInstallation, OwnedFirefox, ProfileKind, ProfileLease},
};
use std::{path::Path, sync::atomic::AtomicBool, time::Duration};

/// (id, address, seconds to let the page's tests finish, script that returns the lines worth reading)
const SITES: [(&str, &str, u64, &str); 5] = [
    (
        "local",
        "about:blank",
        1,
        r#"JSON.stringify({webdriver: navigator.webdriver, hasWebdriver: 'webdriver' in navigator, ownProp: Object.getOwnPropertyNames(navigator), protoDesc: Object.getOwnPropertyDescriptor(Navigator.prototype,'webdriver') ? 'getter' : 'none', chromeKeys: window.chrome ? Object.keys(window.chrome) : null, outer: [outerWidth, outerHeight], inner: [innerWidth, innerHeight], screen: [screen.width, screen.height, screen.availWidth, screen.availHeight], dpr: devicePixelRatio, cores: navigator.hardwareConcurrency, mem: navigator.deviceMemory, langs: navigator.languages, plugins: navigator.plugins.length})"#,
    ),
    (
        "sannysoft",
        "https://bot.sannysoft.com/",
        8,
        r#"Array.from(document.querySelectorAll('table tr')).map(r => (r.querySelector('td.failed') || /failed/i.test(r.className) ? 'FAIL ' : '     ') + r.innerText.replace(/\s+/g,' ').slice(0,160)).join('\n')"#,
    ),
    (
        "incolumitas",
        "https://bot.incolumitas.com/",
        40,
        r#"document.body.innerText.replace(/[ \t]+/g,' ').slice(0,6000)"#,
    ),
    (
        "creepjs",
        "https://abrahamjuliot.github.io/creepjs/",
        30,
        r#"document.body.innerText.replace(/[ \t]+/g,' ').slice(0,5000)"#,
    ),
    (
        "deviceinfo",
        "https://deviceandbrowserinfo.com/are_you_a_bot",
        15,
        r#"document.body.innerText.replace(/[ \t]+/g,' ').slice(0,4000)"#,
    ),
];

fn main() {
    let mut args = std::env::args().skip(1);
    let engine = args.next().unwrap_or_else(|| "chrome".into());
    let wanted: Vec<String> = args.collect();
    let out = Path::new(".local/builds/work/detect").join(&engine);
    std::fs::create_dir_all(&out).expect("output folder");
    let runtime = Path::new(".local/builds/work/detect-runtime");
    std::fs::create_dir_all(runtime).expect("work folder");
    let runtime = runtime.canonicalize().expect("work folder path");
    let lease = ProfileLease::temporary(&runtime).expect("temporary profile");
    let cancelled = AtomicBool::new(false);
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
    if std::env::var_os("CEH_ARRANGE").is_some() {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.subsec_nanos());
        let mut seed = u64::from(nanos) | 1;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % 10_000) as f64 / 10_000.0
        };
        let samples = [0.9, next(), next(), next()];
        println!("arrange_window({samples:?}) -> {:?}", browser.arrange_window(samples));
    }
    let context = browser.context().to_owned();
    for (id, url, wait, script) in SITES {
        if !wanted.is_empty() && !wanted.iter().any(|w| w == id) {
            continue;
        }
        println!("== {engine}: {id} ({url})");
        let navigated = browser
            .connection()
            .call("browsingContext.navigate", serde_json::json!({"context": context, "url": url, "wait": "none"}));
        if let Err(error) = navigated {
            println!("   navigate failed: {error:?}");
            continue;
        }
        std::thread::sleep(Duration::from_secs(wait));
        let text = match browser.connection().call(
            "script.evaluate",
            serde_json::json!({"expression": script, "target": {"context": context}, "awaitPromise": false}),
        ) {
            Ok(value) => value["result"]["value"].as_str().unwrap_or("?").to_owned(),
            Err(error) => format!("evaluate failed: {error:?}"),
        };
        std::fs::write(out.join(format!("{id}.txt")), &text).expect("write text");
        if let Ok(shot) = browser.connection().call(
            "browsingContext.captureScreenshot",
            serde_json::json!({"context": context, "origin": "document"}),
        ) && let Some(data) = shot["data"].as_str()
        {
            // The picture is kept as base64 text (no decoder is linked here): `base64 -D` turns it into the png.
            let _ = std::fs::write(out.join(format!("{id}.png.b64")), data);
        }
        let flagged: Vec<&str> = text.lines().filter(|l| l.starts_with("FAIL")).collect();
        println!("   {} characters saved, {} flagged lines", text.len(), flagged.len());
        for line in flagged.iter().take(12) {
            println!("   {line}");
        }
    }
    let _ = browser.stop();
}
