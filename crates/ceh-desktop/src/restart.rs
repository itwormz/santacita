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
/// The reason of a new browser after the browser could not verify the site's certificate.
pub const CERTIFICATE_RESTART: &str = "браузер не проверил сертификат сайта — браузер перезапущен, профиль сохранён";
impl RestartRequest {
    pub fn from_action(action: &RecoveryAction, page: PageKind) -> Option<Self> {
        let RecoveryAction::Restart { clear_profile, reopen } = *action else { return None };
        let reason = match (page, clear_profile, reopen) {
            (PageKind::Blocked, ..) => "блокировка сайта",
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
