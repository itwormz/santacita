//! How the owned Firefox is controlled without reporting `navigator.webdriver === true`.
//! Preferred: restart the Remote Agent in Firefox's own "remote control" mode (`isBrowserAutomation:false`),
//! where `webdriver` is natively `false`. If the installed Firefox cannot do it, a Proxy mask (v2) is used.
//! Measured on Linux (headless, 2026-11-15): Firefox 155 and 160 switch; 150-154 have `startAtRuntime` but no
//! `stopAtRuntime` (`unsupported`); 145 and older give the chrome scope no system access (`no_system_access`).
use serde_json::json;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebdriverMode {
    RemoteControl,
    Mask,
    /// Chrome-family browsers: `--disable-blink-features=AutomationControlled` already gives a native `false`.
    NotNeeded,
}
impl WebdriverMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::RemoteControl => "remote_control",
            Self::Mask => "mask",
            Self::NotNeeded => "not_needed",
        }
    }
}
/// Why the remote control mode was not used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fallback {
    NoSystemAccess,
    Unsupported,
    RestartFailed,
    Timeout,
}
impl Fallback {
    pub fn name(self) -> &'static str {
        match self {
            Self::NoSystemAccess => "no_system_access",
            Self::Unsupported => "unsupported",
            Self::RestartFailed => "restart_failed",
            Self::Timeout => "timeout",
        }
    }
}
/// Parses `"<port> <isBrowserAutomationRunning>"` written by the restart script. Anything but a valid
/// port with the automation flag off is a failed restart.
pub fn parse_port_report(text: &str) -> Result<u16, Fallback> {
    let mut parts = text.split_whitespace();
    let (Some(port), Some(automation), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(Fallback::RestartFailed);
    };
    match (port.parse::<u16>(), automation) {
        (Ok(port), "false") if port != 0 => Ok(port),
        _ => Err(Fallback::RestartFailed),
    }
}
/// Chrome-context script: schedules the restart (the agent that runs this call stops, so it answers first).
/// `unsupported` when this Firefox has no `startAtRuntime`; otherwise `scheduled`. The only interpolated
/// value is the report file path, JSON-encoded.
pub fn restart_script(report_path: &str) -> String {
    let path = json!(report_path);
    format!(
        "(() => {{ const {{ RemoteAgent }} = ChromeUtils.importESModule('chrome://remote/content/components/RemoteAgent.sys.mjs');\n\
         if (typeof RemoteAgent.startAtRuntime !== 'function' || typeof RemoteAgent.stopAtRuntime !== 'function') return 'unsupported';\n\
         setTimeout(async () => {{ try {{ await RemoteAgent.stopAtRuntime(); const p = await RemoteAgent.startAtRuntime({{ isBrowserAutomation: false }});\n\
         await IOUtils.writeUTF8({path}, String(p > 0 ? p : RemoteAgent.port) + ' ' + RemoteAgent.isBrowserAutomationRunning); }}\n\
         catch (e) {{ await IOUtils.writeUTF8({path}, 'ERR ' + e); }} }}, 300); return 'scheduled'; }})()"
    )
}
/// Fallback preload: the getter is a Proxy over the native one and `Function.prototype.toString` answers with
/// the native source for the getter and for itself, so the replacement does not show in a `toString` check.
/// Residual trace: the stack of an error thrown inside a Proxy trap may show a frame of this script.
pub const WEBDRIVER_MASK_V2: &str = "() => { const proto = Function.prototype, nativeToString = proto.toString; const d = Object.getOwnPropertyDescriptor(Navigator.prototype, 'webdriver'); if (!d || !d.get) return; const getter = new Proxy(d.get, { apply: () => false }); const sources = new WeakMap([[getter, nativeToString.call(d.get)]]); const toString = new Proxy(nativeToString, { apply(target, self, args) { return sources.has(self) ? sources.get(self) : Reflect.apply(target, self, args); } }); sources.set(toString, nativeToString.call(nativeToString)); Object.defineProperty(proto, 'toString', { ...Object.getOwnPropertyDescriptor(proto, 'toString'), value: toString }); Object.defineProperty(Navigator.prototype, 'webdriver', { ...d, get: getter }); }";
