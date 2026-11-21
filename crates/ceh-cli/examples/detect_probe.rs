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
        "https://example.com/",
        1,
        r#"(() => { const code = "(() => { const c = document.createElement('canvas'); c.width = 200; c.height = 40; const x = c.getContext('2d'); x.font = '16px Arial'; x.fillStyle = '#c33'; x.fillText('Cwm fjordbank glyphs vext quiz', 4, 24); let h = 0; const u = c.toDataURL(); for (let i = 0; i < u.length; i++) h = (h * 31 + u.charCodeAt(i)) | 0; document.documentElement.setAttribute('data-main', JSON.stringify({noise: globalThis.__ceh_noise, hash: h, ts: Function.prototype.toString.call(HTMLCanvasElement.prototype.toDataURL), ts2: Function.prototype.toString.call(Function.prototype.toString)})); })()"; const el = document.createElement('script'); el.textContent = code; document.documentElement.appendChild(el); el.remove(); return JSON.stringify({main: document.documentElement.getAttribute('data-main'), webdriver: navigator.webdriver, outer: [outerWidth, outerHeight], inner: [innerWidth, innerHeight], dpr: devicePixelRatio, cores: navigator.hardwareConcurrency}); })()"#,
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
        if std::env::var_os("CEH_MAXIMIZE").is_some() {
            // Start from a maximized window to see that the plan restores it before sizing it.
            let windows = browser.connection().call("browser.getClientWindows", serde_json::json!({}));
            if let Some(id) = windows.ok().and_then(|w| w["clientWindows"][0]["clientWindow"].as_str().map(str::to_owned)) {
                let state = browser.connection().call("browser.setClientWindowState", serde_json::json!({"clientWindow": id, "state": "maximized"}));
                println!("maximized first -> {:?}", state.map(|w| w["state"].clone()));
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        let samples = [0.9, next(), next(), next()];
        println!("arrange_window({samples:?}) -> {:?}", browser.arrange_window(samples));
    }
    if let Some(path) = std::env::var_os("CEH_PRELOAD_JS") {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.subsec_nanos());
        let source = std::fs::read_to_string(path).expect("preload script").replace("__SEED__", &nanos.to_string());
        let added = browser
            .connection()
            .call("script.addPreloadScript", serde_json::json!({"functionDeclaration": format!("() => {{ {source} }}")}));
        println!("preload script: {}", if added.is_ok() { "added".into() } else { format!("{added:?}") });
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
            // A page script that threw answers with the exception and no value: keep the reason in the file.
            Ok(value) if value["type"] == "exception" => format!("EXCEPTION {}", value["exceptionDetails"]),
            Ok(value) => value["result"]["value"].as_str().unwrap_or("?").to_owned(),
            Err(error) => format!("evaluate failed: {error:?}"),
        };
        std::fs::write(out.join(format!("{id}.txt")), &text).expect("write text");
        // A picture left from an earlier run would pass for this run's: remove it before the capture.
        let picture = out.join(format!("{id}.png.b64"));
        let _ = std::fs::remove_file(&picture);
        match browser.connection().call(
            "browsingContext.captureScreenshot",
            serde_json::json!({"context": context, "origin": "document"}),
        ) {
            Ok(shot) => match shot["data"].as_str() {
                // The picture is kept as base64 text (no decoder is linked here): `base64 -D` turns it into the png.
                Some(data) => {
                    if let Err(error) = std::fs::write(&picture, data) {
                        println!("   screenshot not saved: {error}");
                    }
                }
                None => println!("   screenshot has no data"),
            },
            Err(error) => println!("   screenshot failed: {error:?}"),
        }
        let flagged: Vec<&str> = text.lines().filter(|l| l.starts_with("FAIL")).collect();
        println!("   {} characters saved, {} flagged lines", text.len(), flagged.len());
        for line in flagged.iter().take(12) {
            println!("   {line}");
        }
    }
    let _ = browser.stop();
}
