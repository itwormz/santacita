//! Python `AssistRestartRequested` as the core plans it (`FlowEffect::Recover{Restart}`): a new pass in the
//! same browser window, or a new browser session (profile cleared after a block).
use ceh_core::{model::PageKind, recovery::RecoveryAction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartKind {
    /// Same window and profile; the flow starts again from the entry page.
    NewPass,
    /// The browser is closed (profile cleared when asked) and a new one is started.
    NewSession,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestartRequest {
    pub reason: &'static str,
    pub clear_profile: bool,
    pub reopen: bool,
}
/// The reason of a new session after the site blocked the browser.
pub const BLOCK_RESTART: &str = "блокировка сайта";
/// Another installed browser for a session after a block: a browser of the other kind first (Firefox for Chrome, Brave
/// or Edge, a Chromium-based one for Firefox), then any other, in the order a new profile picks its browser.
pub fn other_browser<'a>(current: &str, installed: &[&'a str]) -> Option<&'a str> {
    const ORDER: [&str; 5] = ["chrome", "brave", "edge", "chromium", "firefox"];
    let gecko = |id: &str| id == "firefox";
    let mut candidates: Vec<&'a str> = installed.iter().copied().filter(|id| *id != current).collect();
    candidates.sort_by_key(|id| (gecko(id) == gecko(current), ORDER.iter().position(|o| o == id).unwrap_or(ORDER.len())));
    candidates.first().copied()
}
/// The browser of the session that follows a block. `streak` counts the blocks in a row, this one included: the first
/// keeps the browser (its profile is cleared), the second moves to the other browser, the third goes back to the
/// profile's own browser, where the session waits the block delay. A browser that got past a block stays until the
/// next run of blocks moves it.
pub fn browser_after_block(profile: &str, current: &str, other: Option<&str>, streak: usize, changes: bool) -> String {
    let next = match streak {
        _ if !changes => profile,
        2 if current == profile => other.unwrap_or(profile),
        2 => profile,
        streak if streak > 2 => profile,
        _ => current,
    };
    next.to_owned()
}
/// The reason of a new browser after the browser could not verify the site's certificate.
pub const CERTIFICATE_RESTART: &str = "браузер не проверил сертификат сайта — браузер перезапущен, профиль сохранён";
impl RestartRequest {
    pub fn from_action(action: &RecoveryAction, page: PageKind) -> Option<Self> {
        let RecoveryAction::Restart { clear_profile, reopen } = *action else { return None };
        let reason = match (page, clear_profile, reopen) {
            (PageKind::Blocked, ..) => BLOCK_RESTART,
            (PageKind::ServerError, _, true) => "ошибка портала Cl@ve — профиль и cookies сохранены",
            (PageKind::Loading, ..) => "таймаут ответа сайта — новый проход",
            (PageKind::Unreachable, false, true) => CERTIFICATE_RESTART,
            _ => "перезапуск",
        };
        Some(Self { reason, clear_profile, reopen })
    }
    pub fn kind(&self) -> RestartKind {
        if self.clear_profile || self.reopen { RestartKind::NewSession } else { RestartKind::NewPass }
    }
    /// The journal line Python records (`flow_restart`).
    pub fn note(&self) -> String {
        match self.kind() {
            RestartKind::NewPass => format!("Новый проход: {} — окно и профиль сохранены", self.reason),
            RestartKind::NewSession => format!("Новая сессия: {}", self.reason),
        }
    }
}

/// The reason of a new browser after the connection to the old one stopped answering.
pub const CONNECTION_RESTART: &str = "браузер перестал отвечать — браузер перезапущен, профиль сохранён";
/// The reason of a new pass after an action the browser did not complete.
pub const ACTION_FAILURE_PASS: &str = "действие не выполнено — новый проход";

/// What the search does after an action or a page read failed in the browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureStep {
    /// Same window and profile, a fresh flow from the entry page (Python: "таймаут действия — новый проход").
    NewPass,
    /// The connection to the browser is lost: a new browser with the same profile.
    NewBrowser,
    /// Stop and leave the window to the person.
    Stop,
}
/// Python went on after an action that failed in the browser: it read the page again and started a new pass. The search
/// does the same, a few times in a row at most: a failure that keeps coming back is not a passing one and stops the
/// search as before.
#[derive(Debug, Default)]
pub struct ActionFailure {
    recent: Vec<std::time::Instant>,
}
impl ActionFailure {
    pub const MAX: usize = 3;
    /// Recoveries further apart than this are not one run of failures.
    pub const WINDOW: std::time::Duration = std::time::Duration::from_secs(15 * 60);
    /// `connected`: the browser still answers. `page`: the last page the search saw.
    pub fn decide(&mut self, connected: bool, page: Option<PageKind>, now: std::time::Instant) -> FailureStep {
        // Without a connection the page can no longer be read: a window that may show a booking, or where the person
        // signs in, is never closed.
        if !connected
            && matches!(page, Some(PageKind::Office | PageKind::Contact | PageKind::Booking | PageKind::Captcha | PageKind::AuthWait))
        {
            return FailureStep::Stop;
        }
        self.recent.retain(|at| now.saturating_duration_since(*at) < Self::WINDOW);
        if self.recent.len() >= Self::MAX {
            return FailureStep::Stop;
        }
        self.recent.push(now);
        if connected { FailureStep::NewPass } else { FailureStep::NewBrowser }
    }
}
