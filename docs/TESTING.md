# What is tested, and how

Four suites. The first three run anywhere; the fourth needs Linux or Windows,
because WebDriver cannot drive the macOS webview.

| Suite | Command | Proves |
|---|---|---|
| JavaScript | `npm test` | The bridge's logic against a stand-in for the shell; the CLI; that versions, READMEs and types agree with the source; that the published types compile |
| Rust | `cargo test` (in `src-tauri`) | Path rules, the security boundaries, config loading, process management |
| Ruby | `bundle exec rake test` (in `turbo_desktop-rails`) | The gem's helpers, endpoints and generator |
| End to end | `npm run test:e2e` | The real shell showing a real Turbo app in a real window |

The unit suites passed while the app could not open from a fresh scaffold and
a modal rule also navigated the window underneath. What a person sees is
only checked by the fourth, so a change to how the app opens, navigates or
quits is not done until it is covered there.

## What the end-to-end suite covers

| Promise in the documentation | Test |
|---|---|
| Opening the app starts the server | `opening the app starts its server` |
| The server sees the desktop app's user agent | `the server sees requests as coming from the desktop app` |
| Rules come from the server | `the rules are fetched once the server the app started is up` |
| `TurboDesktop.platform` | `the page knows which platform it is on` |
| `default` navigates in place, through Turbo | `an ordinary link navigates…`, `navigating with Turbo does not reload…` |
| `data-turbo-action="replace"` is kept | `a link can ask to replace the history entry` |
| `modal` opens a modal, and only a modal | `a modal rule opens a modal and leaves the main window where it was` |
| A rule's `width` and `height` | `a modal is sized by its rule` |
| `isModal`, `windowLabel` | the modal and new-window tests |
| `recede()`, `refresh()`, `resume()`, `closeModal()` | one test each |
| `new_window` | `a new_window rule opens a window of its own` |
| `replace` | `a replace rule shows the page without adding to the history` |
| `none` | `a rule of none leaves the link to a bridge component` |
| External links leave the app alone | `a link to another site leaves the app where it is` |
| Bridge components keep their names | `two components on a page each speak for themselves` |
| Notifications | `a notification is shown, or the page is told it cannot be`, `a component saying goodbye is not shown…` |
| Badge | `the badge is set and cleared` |
| Menu items | `a menu item a page registers is in the menu bar`, and replacing and removing one |
| Components declared in the markup | `an element that declares a menu item gets one, for as long as its page is shown` |
| Global shortcuts | `a global shortcut is registered…`, `something that is not a shortcut is refused` |
| Launch at login | `launch at login can be turned on and off` |
| Clipboard | `clipboard text survives a write/read round trip` |
| A picked path is granted | `a save-dialog pick makes the path writable and readable` |
| The filesystem is closed by default | `a path nobody picked is still refused`, `the filesystem is closed outside…` |
| Sudo is off by default | `sudo is off unless the app turns it on` |
| Shell commands stream output | `a shell command streams its output back` |
| Dev Inspector | `the Dev Inspector loads and opens with its shortcut` |
| Failed visits are reported | `a page the server fails on is reported…` |
| Connection loss and recovery | `the app notices its server going away, and coming back` |
| Quitting stops the server | `quitting the app stops the server it started` |

## What has to be checked by hand

These involve the operating system itself, which no driver reaches. Check them
on macOS before a release, in a scaffolded app (`npx turbo-desktop new`).

| # | Do | Expect |
|---|---|---|
| 1 | Drag a file from the Finder onto the window | `turbo-desktop:drop` fires with the file's real path; the file can be read |
| 2 | Drag a folder onto the window | Files inside it can be read |
| 3 | Declare a file type, build, double-click such a file | The app opens and `turbo-desktop:file-open` fires, also from a cold start |
| 4 | Open `your-app://some/path` from a terminal with `open` | The app comes forward and visits `/some/path` |
| 5 | Send a notification | It appears in Notification Centre; the first time, macOS asks permission |
| 6 | Use the native open and save dialogs | The dialog appears; cancelling is handled |
| 7 | Register a menu item and a global shortcut | The item is in the menu bar; the shortcut fires with the app in the background |
| 8 | Set a badge count | The dock icon shows it |
| 9 | Look at the menu bar | The tray icon is there, and its menu works |
| 10 | Quit with Cmd+Q, with a server the app started | The port is free afterwards |
| 11 | Resize the window, quit, open again | It reopens at that size |
| 12 | Enable sudo with one allowed command, run it | The app shows the command and asks, then the system prompt appears |
| 13 | Leave the app in the background past `refresh_after_seconds` | It refreshes on return, unless the focus is in a field |
| 14 | Press Cmd+Shift+D | The Dev Inspector opens and logs bridge traffic |
| 15 | `npx turbo-desktop build`, open the built app | It starts, reads its bundled config, starts the server |

Windows has no automated end-to-end run yet. The Rust suite runs there; the
CLI and the shell's behaviour in a window do not.
