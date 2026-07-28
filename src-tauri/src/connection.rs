//! Reachability of the app server, and the vocabulary for reporting failures.
//!
//! Mirrors Hotwire Native's error model (`TurboError` on turbo-ios, `VisitError`
//! on turbo-android): the shell names *why* a visit failed, hands the reason to
//! the web layer, and lets the app decide how to present it. The default
//! presentation is a retry affordance, same as the mobile shells.

use serde::{Deserialize, Serialize};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// How long a reachability probe may take before it counts as a failure.
const PROBE_TIMEOUT: Duration = Duration::from_millis(500);

/// Consecutive failed probes before the app is considered offline.
///
/// One failure is not enough: a single dropped probe during a deploy or a
/// laptop waking up should not pull the page out from under someone.
const FAILURES_BEFORE_OFFLINE: u32 = 2;

/// Why a visit could not be completed.
///
/// The variants match Hotwire Native so the same words mean the same thing
/// across the mobile and desktop shells.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum VisitError {
    /// The server could not be reached at all.
    NetworkFailure,
    /// The server accepted the connection but did not answer in time.
    TimeoutFailure,
    /// The server answered with an error status.
    HttpFailure { status: u16 },
    /// The page itself failed to load.
    PageLoadFailure,
}

impl VisitError {
    /// Stable identifier, used in the error page's query string and in the
    /// events the web layer listens for.
    pub fn slug(&self) -> &'static str {
        match self {
            Self::NetworkFailure => "network_failure",
            Self::TimeoutFailure => "timeout_failure",
            Self::HttpFailure { .. } => "http_failure",
            Self::PageLoadFailure => "page_load_failure",
        }
    }
}

/// What a probe changed, if anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transition {
    /// Enough probes failed in a row to call it offline.
    WentOffline(VisitError),
    /// The server answered again.
    CameOnline,
    /// Nothing worth telling the web layer about.
    Unchanged,
}

/// Turns a stream of probe results into offline/online transitions.
///
/// Deliberately separate from the probing itself so the hysteresis can be
/// tested without a socket.
#[derive(Debug, Default)]
pub struct ConnectionMonitor {
    consecutive_failures: u32,
    offline: bool,
}

impl ConnectionMonitor {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub fn is_offline(&self) -> bool {
        self.offline
    }

    /// Feed in one probe result and find out what changed.
    pub fn record(&mut self, reachable: bool) -> Transition {
        if reachable {
            self.consecutive_failures = 0;

            if self.offline {
                self.offline = false;
                return Transition::CameOnline;
            }
            return Transition::Unchanged;
        }

        self.consecutive_failures = self.consecutive_failures.saturating_add(1);

        if !self.offline && self.consecutive_failures >= FAILURES_BEFORE_OFFLINE {
            self.offline = true;
            return Transition::WentOffline(VisitError::NetworkFailure);
        }

        Transition::Unchanged
    }
}

/// Can we open a TCP connection to the app server?
///
/// A plain connect rather than an HTTP request: it is cheap enough to run on a
/// timer, needs no client, and answers the only question being asked — is
/// anything listening.
pub fn server_is_reachable(url: &url::Url) -> bool {
    let (Some(host), Some(port)) = (url.host_str(), url.port_or_known_default()) else {
        return false;
    };

    match (host, port).to_socket_addrs() {
        Ok(addrs) => addrs
            .into_iter()
            .any(|addr| TcpStream::connect_timeout(&addr, PROBE_TIMEOUT).is_ok()),
        Err(_) => false,
    }
}

/// Try the app again now, rather than waiting for the next scheduled probe.
///
/// This is what the error page's retry button calls — the desktop counterpart of
/// the retry handler Hotwire Native hands to a failed visitable. Returns whether
/// the server answered; when it did, the window has already been sent back.
#[tauri::command]
pub async fn retry_connection(
    app: tauri::AppHandle,
    webview: tauri::Webview,
) -> Result<bool, String> {
    use tauri::Manager;

    crate::security::ensure_trusted_caller(&app, &webview)?;

    let config = app.state::<crate::window::TurboDesktopConfig>();
    let url: url::Url = config
        .server_url
        .parse()
        .map_err(|e| format!("Invalid server URL: {}", e))?;

    let probe_url = url.clone();
    let reachable = tokio::task::spawn_blocking(move || server_is_reachable(&probe_url))
        .await
        .unwrap_or(false);

    if reachable {
        if let Some(window) = app.get_webview_window("main") {
            window
                .navigate(url)
                .map_err(|e| format!("Could not navigate to the app: {}", e))?;
        }
    }

    Ok(reachable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_failure_is_not_enough_to_go_offline() {
        let mut monitor = ConnectionMonitor::new();

        assert_eq!(monitor.record(false), Transition::Unchanged);
        assert!(!monitor.is_offline());
    }

    #[test]
    fn consecutive_failures_go_offline_once() {
        let mut monitor = ConnectionMonitor::new();

        monitor.record(false);
        assert_eq!(
            monitor.record(false),
            Transition::WentOffline(VisitError::NetworkFailure)
        );
        assert!(monitor.is_offline());

        // Still offline, but there is nothing new to announce.
        assert_eq!(monitor.record(false), Transition::Unchanged);
    }

    #[test]
    fn a_success_between_failures_resets_the_count() {
        let mut monitor = ConnectionMonitor::new();

        monitor.record(false);
        assert_eq!(monitor.record(true), Transition::Unchanged);
        assert_eq!(monitor.record(false), Transition::Unchanged);
        assert!(!monitor.is_offline(), "the run of failures was broken");
    }

    #[test]
    fn recovery_is_announced_once() {
        let mut monitor = ConnectionMonitor::new();

        monitor.record(false);
        monitor.record(false);

        assert_eq!(monitor.record(true), Transition::CameOnline);
        assert_eq!(monitor.record(true), Transition::Unchanged);
        assert!(!monitor.is_offline());
    }

    #[test]
    fn error_slugs_match_the_names_the_web_layer_listens_for() {
        assert_eq!(VisitError::NetworkFailure.slug(), "network_failure");
        assert_eq!(VisitError::TimeoutFailure.slug(), "timeout_failure");
        assert_eq!(VisitError::HttpFailure { status: 500 }.slug(), "http_failure");
        assert_eq!(VisitError::PageLoadFailure.slug(), "page_load_failure");
    }

    #[test]
    fn errors_serialize_with_their_type_and_detail() {
        let json = serde_json::to_value(VisitError::HttpFailure { status: 503 }).unwrap();

        assert_eq!(json["type"], "http_failure");
        assert_eq!(json["status"], 503);
    }

    #[test]
    fn an_unresolvable_host_is_not_reachable() {
        let url = url::Url::parse("http://turbo-desktop.invalid:3000").unwrap();
        assert!(!server_is_reachable(&url));
    }
}
