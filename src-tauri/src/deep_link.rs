//! Opening the app at a particular page from outside it.
//!
//! A link like `your-app://orders/123` — from an email, a calendar entry, or
//! another application — lands here and becomes a visit to the matching page on
//! your server. This is the desktop counterpart of Hotwire Native's universal
//! links, minus the domain verification those get from the OS.
//!
//! The scheme is registered per app rather than shared across everything built
//! on this shell, because no desktop OS arbitrates duplicate registrations in a
//! way you control: two apps claiming one scheme means links silently open the
//! wrong one.

use tauri::Manager;

/// Turn a deep link into the page on the app server it refers to.
///
/// Everything after the scheme is treated as a path, so `your-app://orders/123`
/// and `your-app:/orders/123` both mean `/orders/123`. The result is resolved
/// against the configured server and rejected if it lands anywhere else — a
/// link arrives from outside the app, so it is not trusted to say where to go.
/// The file a URL names, when it names one.
///
/// macOS hands a file opened with the app to the same place as a link, as a
/// `file:` URL. It is a file to open, not a page to visit.
pub fn opened_file(link: &url::Url) -> Option<std::path::PathBuf> {
    if link.scheme() != "file" {
        return None;
    }
    link.to_file_path().ok()
}

pub fn resolve(server_url: &str, link: &url::Url) -> Result<url::Url, String> {
    if link.scheme() == "file" {
        return Err(format!("Refused: '{}' is a file, not a link", link));
    }

    let server: url::Url = server_url
        .parse()
        .map_err(|e| format!("Invalid server URL: {}", e))?;

    // A custom scheme puts the first segment in the host, so `app://orders/123`
    // parses as host "orders" with path "/123". Stitch them back together.
    let mut path = String::new();
    if let Some(host) = link.host_str() {
        path.push('/');
        path.push_str(host);
    }
    path.push_str(link.path());

    if path.is_empty() || path == "/" {
        return Err("Deep link has no path".to_string());
    }

    let mut target = server
        .join(&path)
        .map_err(|e| format!("Could not resolve '{}': {}", link, e))?;
    target.set_query(link.query());
    target.set_fragment(link.fragment());

    if crate::security::is_trusted_origin(server_url, &target) {
        Ok(target)
    } else {
        Err(format!("Refused: '{}' resolves outside the app", link))
    }
}

/// The link the app was last asked to open, waiting for the page to collect it.
///
/// A link that starts the app arrives before there is a page to give it to,
/// and one that arrives while the server is starting finds the waiting page.
/// It is kept here instead: the shell pings the page, and the page asks for
/// the link once it is the app's own page, on its own startup if the ping
/// came too early.
#[derive(Default)]
pub struct PendingLink(std::sync::Mutex<Option<String>>);

impl PendingLink {
    /// Keep a link for the page. A later one replaces it: it is where the
    /// person asked to go last.
    pub fn keep(&self, target: &str) {
        *self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(target.to_string());
    }

    /// Hand the link over, once.
    pub fn take(&self) -> Option<String> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }
}

/// Handle deep links as they arrive.
pub fn handle(app: &tauri::AppHandle, urls: Vec<url::Url>) {
    let config = app.state::<crate::window::TurboDesktopConfig>();

    let files: Vec<std::path::PathBuf> = urls.iter().filter_map(opened_file).collect();
    if !files.is_empty() {
        handle_files(app, files);
    }

    for link in urls.into_iter().filter(|link| opened_file(link).is_none()) {
        log::info!("Deep link: {}", link);

        let target = match resolve(&config.server_url, &link) {
            Ok(target) => target,
            Err(e) => {
                log::warn!("{}", e);
                continue;
            }
        };

        app.state::<PendingLink>().keep(target.as_str());

        // Ping a loaded page so it collects the link now, and visits through
        // Turbo, so the path configuration still decides how it is presented.
        // A page that is not there yet misses the ping and asks on its own
        // startup instead.
        if let Some(window) = app.get_webview_window("main") {
            crate::window::deliver_to_page(&window, "deep-link-pending", &serde_json::json!({}));
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

/// Files the OS asked the app to open, waiting for the page to collect them.
///
/// A file association launch usually happens before the page has loaded, so a
/// pushed event would land in an empty webview. The paths queue here instead:
/// the shell pings the page, and the page drains the queue through the bridge
/// — on its own startup if the ping arrived too early.
#[derive(Default)]
pub struct PendingOpenedFiles(std::sync::Mutex<Vec<String>>);

/// The files opened in the last moment, so that one reported twice is opened
/// once. macOS reports an opened file as a file and as a URL.
#[derive(Default)]
pub struct RecentlyOpened(std::sync::Mutex<std::collections::HashMap<String, std::time::Instant>>);

impl RecentlyOpened {
    const MOMENT: std::time::Duration = std::time::Duration::from_secs(2);

    /// True the first time a path is seen, and again once a moment has passed.
    pub fn is_new(&self, path: &str, now: std::time::Instant) -> bool {
        let mut seen = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        seen.retain(|_, at| now.saturating_duration_since(*at) < Self::MOMENT);

        if seen.contains_key(path) {
            return false;
        }
        seen.insert(path.to_string(), now);
        true
    }
}

/// Handle files the OS handed to the app — a double-click on an associated
/// type, "Open With…", or a file dropped on the app's icon.
pub fn handle_files(app: &tauri::AppHandle, paths: Vec<std::path::PathBuf>) {
    if paths.is_empty() {
        return;
    }

    // Being asked to open a file is the same consent as picking it in a
    // dialog, so the page can read what it was handed.
    let grants = app.state::<crate::security::UserGrants>();
    let mut opened: Vec<String> = Vec::new();
    let recent = app.state::<RecentlyOpened>();
    let now = std::time::Instant::now();

    for path in &paths {
        let raw = path.to_string_lossy().into_owned();
        if !recent.is_new(&raw, now) {
            continue;
        }
        if path.is_dir() {
            grants.grant_folder(&raw);
        } else {
            grants.grant_file(&raw);
        }
        log::info!("Opening from the OS: {}", raw);
        opened.push(raw);
    }

    if opened.is_empty() {
        return;
    }

    app.state::<PendingOpenedFiles>()
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .extend(opened);

    // Ping a loaded page so it drains the queue now; a page that is not there
    // yet misses the ping and drains on its own startup instead.
    if let Some(window) = app.get_webview_window("main") {
        crate::window::deliver_to_page(&window, "file-open-pending", &serde_json::json!({}));
    }
    bring_forward(app);
}

/// Put the app's window in front: someone asked for the app.
pub fn bring_forward(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Bridge handler: the page collects (and thereby clears) the queued files.
pub fn drain_pending(app: &tauri::AppHandle) -> Vec<String> {
    std::mem::take(
        &mut *app
            .state::<PendingOpenedFiles>()
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    )
}

/// The paths the OS launched the app with, on platforms where an associated
/// file arrives as a plain argument (Windows and Linux; macOS uses an event).
///
/// A path that is not absolute is taken from the directory the launch was
/// made in, which for a second copy of the app is not the directory this one
/// is running in.
pub fn paths_from_launch<I: Iterator<Item = String>>(
    args: I,
    directory: &std::path::Path,
) -> Vec<std::path::PathBuf> {
    args.skip(1)
        .filter(|arg| !arg.starts_with('-'))
        .map(|arg| directory.join(arg))
        .filter(|path| path.exists())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // A second copy of the app is started wherever the person was, and hands
    // its arguments to the first, which is running somewhere else.
    #[test]
    fn a_file_named_from_another_directory_is_found_there() {
        let directory = std::env::temp_dir().join(format!("td-launch-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let file = directory.join("tasks.csv");
        std::fs::write(&file, "title\n").unwrap();

        let args = vec![
            "turbo-desktop".to_string(),
            "--flag".to_string(),
            "tasks.csv".to_string(),
            "missing.csv".to_string(),
            "task-manager://orders/1".to_string(),
        ];
        assert_eq!(paths_from_launch(args.into_iter(), &directory), vec![file.clone()]);

        let absolute = vec!["turbo-desktop".to_string(), file.to_string_lossy().into_owned()];
        assert_eq!(
            paths_from_launch(absolute.into_iter(), std::path::Path::new("/nowhere")),
            vec![file]
        );

        let _ = std::fs::remove_dir_all(&directory);
    }

    // macOS tells the app about an opened file twice over: as a file to
    // open, and as a URL. It is one file, opened once.
    #[test]
    fn a_file_reported_twice_is_opened_once() {
        let recent = RecentlyOpened::default();
        let now = std::time::Instant::now();

        assert!(recent.is_new("/tmp/tasks.csv", now));
        assert!(!recent.is_new("/tmp/tasks.csv", now + std::time::Duration::from_millis(300)));
    }

    #[test]
    fn a_file_opened_again_later_is_opened_again() {
        let recent = RecentlyOpened::default();
        let now = std::time::Instant::now();

        assert!(recent.is_new("/tmp/tasks.csv", now));
        assert!(recent.is_new("/tmp/tasks.csv", now + std::time::Duration::from_secs(5)));
    }

    #[test]
    fn another_file_is_another_file() {
        let recent = RecentlyOpened::default();
        let now = std::time::Instant::now();

        assert!(recent.is_new("/tmp/tasks.csv", now));
        assert!(recent.is_new("/tmp/more.csv", now));
    }

    // macOS hands a file opened with the app to the same place as a link, as
    // a file: URL. Read as a link, its path became a page of the app:
    // GET /Users/someone/Desktop/tasks.csv.
    /// A path of the kind this platform has, in a folder with a space in it.
    fn somewhere(name: &str) -> std::path::PathBuf {
        let folder = if cfg!(windows) {
            r"C:\Users\someone\My Tasks"
        } else {
            "/Users/someone/My Tasks"
        };
        std::path::Path::new(folder).join(name)
    }

    #[test]
    fn a_file_is_a_file_and_not_a_link() {
        let path = somewhere("tasks.csv");
        let opened = url::Url::from_file_path(&path).unwrap();

        assert_eq!(opened.scheme(), "file");
        assert_eq!(opened_file(&opened), Some(path));
    }

    #[test]
    fn a_file_with_spaces_in_its_name_is_still_that_file() {
        let path = somewhere("to do.csv");
        let opened = url::Url::from_file_path(&path).unwrap();

        assert!(opened.as_str().contains("to%20do.csv"));
        assert_eq!(opened_file(&opened), Some(path));
    }

    #[test]
    fn a_link_is_not_a_file() {
        let link = url::Url::parse("task-manager://orders/123").unwrap();

        assert_eq!(opened_file(&link), None);
    }

    #[test]
    fn a_file_is_never_followed_as_a_link() {
        let opened = url::Url::from_file_path(somewhere("tasks.csv")).unwrap();

        assert!(resolve("http://localhost:3000", &opened).is_err());
    }

    // A link that starts the app arrives before there is a page to give it
    // to. It was handed to the empty window and lost.
    #[test]
    fn a_link_is_kept_until_the_page_collects_it() {
        let pending = PendingLink::default();

        pending.keep("http://localhost:3000/orders/123");

        assert_eq!(pending.take().as_deref(), Some("http://localhost:3000/orders/123"));
    }

    #[test]
    fn a_link_is_collected_once() {
        let pending = PendingLink::default();
        pending.keep("http://localhost:3000/orders/123");

        pending.take();

        assert_eq!(pending.take(), None);
    }

    #[test]
    fn the_last_link_is_the_one_that_is_followed() {
        let pending = PendingLink::default();

        pending.keep("http://localhost:3000/orders/1");
        pending.keep("http://localhost:3000/orders/2");

        assert_eq!(pending.take().as_deref(), Some("http://localhost:3000/orders/2"));
    }

    fn link(s: &str) -> url::Url {
        url::Url::parse(s).expect("test link should parse")
    }

    #[test]
    fn only_existing_non_flag_arguments_are_opened_files() {
        let file = crate::test_temp_dir().join("turbo-desktop-assoc.txt");
        std::fs::write(&file, "x").unwrap();

        let args = vec![
            "/usr/bin/app".to_string(),
            "--flag".to_string(),
            file.to_string_lossy().into_owned(),
            "/nonexistent/other.txt".to_string(),
        ];

        assert_eq!(
            paths_from_launch(args.into_iter(), std::path::Path::new("/")),
            vec![file.clone()]
        );
        std::fs::remove_file(&file).ok();
    }

    #[test]
    fn the_first_segment_is_part_of_the_path() {
        // A custom scheme parses that segment as the host, which it is not.
        let target = resolve("https://app.example.com", &link("myapp://orders/123")).unwrap();

        assert_eq!(target.as_str(), "https://app.example.com/orders/123");
    }

    #[test]
    fn a_single_segment_link_still_resolves() {
        let target = resolve("https://app.example.com", &link("myapp://settings")).unwrap();

        assert_eq!(target.as_str(), "https://app.example.com/settings");
    }

    #[test]
    fn queries_and_fragments_survive() {
        let target = resolve(
            "https://app.example.com",
            &link("myapp://search?q=hello#results"),
        )
        .unwrap();

        assert_eq!(target.query(), Some("q=hello"));
        assert_eq!(target.fragment(), Some("results"));
    }

    #[test]
    fn a_link_with_no_path_is_refused() {
        assert!(resolve("https://app.example.com", &link("myapp://")).is_err());
    }

    #[test]
    fn a_link_cannot_send_the_app_somewhere_else() {
        // Deep links arrive from outside, so a link naming another host must not
        // be able to point the app at it.
        let err = resolve(
            "https://app.example.com",
            &link("myapp:https://evil.example.com/steal"),
        )
        .expect_err("an absolute URL to another origin must be refused");

        assert!(err.contains("outside the app"), "unexpected error: {err}");
    }

    #[test]
    fn traversal_cannot_climb_out_of_the_server() {
        let target = resolve("https://app.example.com/", &link("myapp://../../etc/passwd"));

        // Either refused, or normalised back onto the app's own origin.
        if let Ok(url) = target {
            assert_eq!(url.host_str(), Some("app.example.com"));
        }
    }
}
