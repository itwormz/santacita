//! LOCAL humanize probe: serves `support/humanize-probe.html` on 127.0.0.1 and saves what a page sees of
//! mouse, wheel and key input. A person fills it in (`serve`, then open `?label=manual-1`, `manual-2`, ...);
//! `automated <label>` drives the app's own browser with the same plans the app uses (click, wheel, typing).
//! `report` summarises every saved run into `.local/builds/reports/env-probe/humanize.md`. Never contacts the
//! appointment site and collects no personal data.
//! Usage: `cargo run -p ceh-cli --example humanize_probe --release -- serve|automated <label> [runs]|report`
//! (`CHROMIUM=chrome` for Chrome, `MASK=1` for the Firefox mask fallback).
use ceh_browser::firefox::{FirefoxInstallation, OwnedFirefox, ProfileKind, ProfileLease};
use ceh_core::input::{ElementBox, Reach, click_plan, scroll_needed, typing_rhythm, wheel_plan};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::{Read, Write}, net::TcpListener, path::{Path, PathBuf}, sync::atomic::AtomicBool, time::{Duration, Instant}};

const PAGE: &str = include_str!("support/humanize-probe.html");
const PORT: u16 = 47832;
type Fail = Box<dyn std::error::Error>;
fn call(browser: &mut OwnedFirefox, method: &str, params: Value) -> Result<Value, Fail> {
    browser.connection().call(method, params).map_err(|e| format!("{method}: {e:?}").into())
}

fn serve(listener: TcpListener, out: PathBuf) {
    for stream in listener.incoming().flatten() {
        let out = out.clone();
        // One thread per connection: Chrome keeps idle connections open.
        std::thread::spawn(move || handle(stream, &out));
    }
}
fn handle(stream: std::net::TcpStream, out: &Path) {
    {
        let mut stream = stream;
        let mut data = Vec::new();
        let mut buffer = [0u8; 8192];
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        while let Ok(n) = stream.read(&mut buffer) {
            if n == 0 { break; }
            data.extend_from_slice(&buffer[..n]);
            let text = String::from_utf8_lossy(&data).to_string();
            if let Some(split) = text.find("\r\n\r\n") {
                let length = text[..split].lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok())).unwrap_or(0);
                if data.len() >= split + 4 + length { break; }
            }
        }
        let text = String::from_utf8_lossy(&data).to_string();
        let first = text.lines().next().unwrap_or("").to_owned();
        if std::env::var_os("PROBE_DEBUG").is_some() { eprintln!("request: {first:?} ({} bytes)", data.len()); }
        if first.starts_with("POST /report") {
            let label: String = first.split("label=").nth(1).and_then(|r| r.split(' ').next()).unwrap_or("unlabeled").chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
            let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
            let _ = std::fs::write(out.join(format!("humanize-{label}.json")), body);
            println!("saved humanize-{label}.json");
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
        } else {
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", PAGE.len(), PAGE);
        }
    }
}
fn seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)
}
fn evaluate(browser: &mut OwnedFirefox, expression: &str) -> Result<Value, Fail> {
    let context = browser.context().to_owned();
    let result = call(browser, "script.evaluate", json!({"expression":expression,"target":{"context":context},"awaitPromise":false,"resultOwnership":"root"}))?;
    Ok(result)
}
/// Centre, size and room around an element (viewport coordinates).
fn geometry(browser: &mut OwnedFirefox, id: &str) -> Result<(Value, [f64; 6]), Fail> {
    let node = evaluate(browser, &format!("document.getElementById('{id}')"))?;
    let shared = node["result"]["sharedId"].as_str().ok_or("no node")?.to_owned();
    let facts = evaluate(browser, &format!("(()=>{{const r=document.getElementById('{id}').getBoundingClientRect();const w=document.documentElement.clientWidth,h=document.documentElement.clientHeight;const cx=r.left+r.width/2,cy=r.top+r.height/2;return JSON.stringify([cx,w-cx,cy,h-cy,r.width,r.height]);}})()"))?;
    let values: Vec<f64> = serde_json::from_str(facts["result"]["value"].as_str().ok_or("no geometry")?)?;
    Ok((json!({"type":"element","element":{"sharedId":shared}}), [values[0], values[1], values[2], values[3], values[4], values[5]]))
}
fn scroll_to(browser: &mut OwnedFirefox, id: &str) -> Result<(), Fail> {
    let context = browser.context().to_owned();
    for _ in 0..3 {
        let (_, g) = geometry(browser, id)?;
        let needed = scroll_needed(seed(), g[2], g[5], g[2] + g[3]);
        if needed == 0 { return Ok(()); }
        let mut actions = Vec::new();
        for notch in wheel_plan(seed(), needed) {
            actions.push(json!({"type":"scroll","x":300,"y":300,"deltaX":0,"deltaY":notch.delta,"duration":0,"origin":"viewport"}));
            actions.push(json!({"type":"pause","duration":notch.pause_ms}));
        }
        call(browser, "input.performActions", json!({"context":context,"actions":[{"type":"wheel","id":"w","actions":actions}]}))?;
        // Smooth scrolling goes on after the last notch: wait until the element stops moving.
        let mut last = f64::MAX;
        for _ in 0..15 {
            std::thread::sleep(Duration::from_millis(80));
            let (_, now) = geometry(browser, id)?;
            if (now[2] - last).abs() < 0.5 { break; }
            last = now[2];
        }
    }
    Ok(())
}
fn click(browser: &mut OwnedFirefox, id: &str, from: &mut Option<(f64, f64)>) -> Result<(), Fail> {
    let context = browser.context().to_owned();
    let (origin, g) = geometry(browser, id)?;
    let room = |v: f64| (v.floor() as i32 - 2).max(0);
    let reach = Reach { left: room(g[0]), right: room(g[1]), up: room(g[2]), down: room(g[3]) };
    let start = from.map(|(x, y)| ((x - g[0]).round() as i32, (y - g[2]).round() as i32));
    let plan = click_plan(seed(), reach, ElementBox { width: g[4], height: g[5] }, start);
    let mut actions: Vec<Value> = plan.moves.iter().map(|s| json!({"type":"pointerMove","duration":s.ms,"x":s.x,"y":s.y,"origin":origin})).collect();
    actions.extend([json!({"type":"pause","duration":plan.hover_ms}), json!({"type":"pointerDown","button":0}), json!({"type":"pause","duration":plan.press_ms}), json!({"type":"pointerUp","button":0})]);
    call(browser, "input.performActions", json!({"context":context,"actions":[{"type":"pointer","id":"p","parameters":{"pointerType":"mouse"},"actions":actions}]}))?;
    *from = Some((g[0] + plan.end.0 as f64, g[2] + plan.end.1 as f64));
    Ok(())
}
fn type_text(browser: &mut OwnedFirefox, text: &str) -> Result<(), Fail> {
    let context = browser.context().to_owned();
    let rhythm = typing_rhythm(seed(), 90, text.chars().count());
    std::thread::sleep(Duration::from_millis(rhythm.first_ms));
    for (c, stroke) in text.chars().zip(&rhythm.keys) {
        let actions = vec![json!({"type":"keyDown","value":c.to_string()}), json!({"type":"pause","duration":stroke.hold_ms}), json!({"type":"keyUp","value":c.to_string()})];
        call(browser, "input.performActions", json!({"context":context,"actions":[{"type":"key","id":"k","actions":actions}]}))?;
        std::thread::sleep(Duration::from_millis(stroke.pause_ms));
    }
    Ok(())
}
fn automated(label: &str, runs: usize, out: &Path) -> Result<(), Fail> {
    let runtime = Path::new(".local/builds/work/env-probe");
    std::fs::create_dir_all(runtime)?;
    let runtime = runtime.canonicalize()?;
    let mut browser = if let Ok(kind) = std::env::var("CHROMIUM") {
        let kind = if kind == "brave" { ProfileKind::Brave } else { ProfileKind::Chrome };
        let installation = ceh_browser::chromium::ChromiumInstallation::discover(kind).map_err(|_| "no chromium")?;
        OwnedFirefox::start_chromium(&installation, ProfileLease::temporary_kind(&runtime, kind).map_err(|_| "profile")?, &AtomicBool::new(false)).map_err(|e| format!("start {e:?}"))?
    } else {
        let installation = FirefoxInstallation::discover().map_err(|_| "no firefox")?;
        let lease = ProfileLease::temporary(&runtime).map_err(|_| "profile")?;
        if std::env::var("MASK").is_ok() { OwnedFirefox::start_masked(&installation, lease, false, &AtomicBool::new(false)) } else { OwnedFirefox::start(&installation, lease, false, &AtomicBool::new(false)) }.map_err(|e| format!("start {e:?}"))?
    };
    eprintln!("webdriver mode: {}", browser.webdriver_mode().name());
    let mut cursor = None;
    for run in 1..=runs {
        let context = browser.context().to_owned();
        let url = format!("http://127.0.0.1:{PORT}/?label={label}-{run}");
        call(&mut browser, "browsingContext.navigate", json!({"context":context,"url":url,"wait":"complete"}))?;
        std::thread::sleep(Duration::from_millis(800));
        scroll_to(&mut browser, "f")?;
        click(&mut browser, "f", &mut cursor)?;
        type_text(&mut browser, "Y1234567X")?;
        scroll_to(&mut browser, "b")?;
        click(&mut browser, "b", &mut cursor)?;
        let file = out.join(format!("humanize-{label}-{run}.json"));
        let deadline = Instant::now() + Duration::from_secs(10);
        while !file.exists() && Instant::now() < deadline { std::thread::sleep(Duration::from_millis(100)); }
        println!("run {run}: {}", if file.exists() { "report saved" } else { "NO report" });
        if !file.exists() {
            let state = evaluate(&mut browser, "JSON.stringify({y:scrollY,value:document.getElementById('f').value,b:document.getElementById('b').getBoundingClientRect().toJSON(),vh:innerHeight,active:document.activeElement&&document.activeElement.id,title:document.title})")?;
            eprintln!("  state: {}", state["result"]["value"]);
        }
    }
    let _ = browser.stop();
    Ok(())
}
fn quantiles(mut values: Vec<f64>) -> String {
    if values.is_empty() { return "—".into(); }
    values.sort_by(|a, b| a.total_cmp(b));
    let at = |p: f64| values[((values.len() - 1) as f64 * p).round() as usize];
    format!("{:.0} / {:.0} / {:.0} (min {:.0}, max {:.0}, n={})", at(0.1), at(0.5), at(0.9), values[0], values[values.len() - 1], values.len())
}
fn report(out: &Path) -> Result<(), Fail> {
    let mut groups: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for entry in std::fs::read_dir(out)?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(stem) = name.strip_prefix("humanize-").and_then(|n| n.strip_suffix(".json")) else { continue };
        let group = stem.rsplit_once('-').map_or(stem, |(g, _)| g).to_owned();
        if let Ok(value) = serde_json::from_str::<Value>(&std::fs::read_to_string(entry.path())?) { groups.entry(group).or_default().push(value); }
    }
    type Extract = Box<dyn Fn(&Value) -> Vec<f64>>;
    let rows: [(&str, Extract); 8] = [
        ("mousemove gap, ms (p10 / median / p90)", Box::new(|r| nums(&r["mousemove_gaps_ms"]))),
        ("click hover before press, ms", Box::new(|r| r["clicks"].as_array().into_iter().flatten().filter_map(|c| c["hover_ms"].as_f64()).collect())),
        ("button hold, ms", Box::new(|r| r["clicks"].as_array().into_iter().flatten().filter_map(|c| c["hold_ms"].as_f64()).collect())),
        ("click offsetX", Box::new(|r| r["clicks"].as_array().into_iter().flatten().filter_map(|c| c["offsetX"].as_f64()).collect())),
        ("click screenX - clientX", Box::new(|r| r["clicks"].as_array().into_iter().flatten().filter_map(|c| c["screenDX"].as_f64()).collect())),
        ("key hold, ms", Box::new(|r| nums(&r["key_holds_ms"]))),
        ("key gap, ms", Box::new(|r| nums(&r["key_gaps_ms"]))),
        ("wheel events per run", Box::new(|r| vec![r["wheel_events"].as_f64().unwrap_or(0.0)])),
    ];
    let mut md = String::from("# Humanize probe: person vs application\n\nEach cell: p10 / median / p90 (min, max, samples). A value that is constant for the app but varies for the person is a finding.\n\n| Measure |");
    for group in groups.keys() { md.push_str(&format!(" {group} ({} runs) |", groups[group].len())); }
    md.push_str("\n|---|");
    md.push_str(&"---|".repeat(groups.len()));
    md.push('\n');
    for (name, extract) in &rows {
        md.push_str(&format!("| {name} |"));
        for runs in groups.values() { md.push_str(&format!(" {} |", quantiles(runs.iter().flat_map(extract).collect()))); }
        md.push('\n');
    }
    md.push_str("| scroll events before any wheel |");
    for runs in groups.values() { md.push_str(&format!(" {} |", runs.iter().filter_map(|r| r["scroll_before_wheel"].as_u64()).sum::<u64>())); }
    md.push_str("\n| typed text correct |");
    for runs in groups.values() { md.push_str(&format!(" {}/{} |", runs.iter().filter(|r| r["typed"] == "Y1234567X").count(), runs.len())); }
    md.push('\n');
    std::fs::write(out.join("humanize.md"), &md)?;
    println!("{md}");
    Ok(())
}
fn nums(value: &Value) -> Vec<f64> {
    value.as_array().into_iter().flatten().filter_map(Value::as_f64).collect()
}
fn main() -> Result<(), Fail> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "serve".into());
    std::fs::create_dir_all(".local/builds/reports/env-probe")?;
    let out = Path::new(".local/builds/reports/env-probe").canonicalize()?;
    if mode == "report" { return report(&out); }
    let listener = TcpListener::bind(("127.0.0.1", PORT))?;
    println!("probe page: http://127.0.0.1:{PORT}/?label=manual-1 (reports go to {})", out.display());
    if mode == "automated" {
        let label = std::env::args().nth(2).unwrap_or_else(|| "automated".into());
        let runs = std::env::args().nth(3).and_then(|r| r.parse().ok()).unwrap_or(8);
        std::thread::spawn({ let out = out.clone(); move || serve(listener, out) });
        automated(&label, runs, &out)
    } else {
        serve(listener, out);
        Ok(())
    }
}
