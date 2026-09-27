// A small Turbo app for the end-to-end tests: real links, a real path
// configuration, and the Dev Inspector's scripts, served the way the Rails gem
// serves them. Dependency-free apart from Turbo itself.
//
// Started by the shell, from the `server.command` in the test's config — the
// tests never start it themselves, because whether the shell can is one of the
// things being tested.
import { createServer } from "node:http";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..", "..");
const port = Number(process.env.PORT || 3211);

const TURBO = readFileSync(
  join(repoRoot, "node_modules", "@hotwired", "turbo", "dist", "turbo.es2017-umd.js")
);

const RULES = {
  settings: { screenshots_enabled: false },
  rules: [
    { patterns: ["/"], properties: { presentation: "default" } },
    { patterns: ["/new$", "/edit$"], properties: { presentation: "modal", width: 500, height: 400 } },
    { patterns: ["/reports/"], properties: { presentation: "new_window" } },
    { patterns: ["/dashboard$"], properties: { presentation: "replace" } },
    { patterns: ["/handled-by-a-component$"], properties: { presentation: "none" } },
  ],
};

const INSPECTOR_MODULES = ["state.js", "panel.js", "bridge-tap.js", "catalog.js"];

const requests = [];

function page(title, body) {
  return `<!doctype html>
<html>
  <head>
    <title>${title}</title>
    <meta name="turbo-desktop-inspector" content="enabled" data-inspector-url="/turbo-desktop/inspector.js">
    <!-- Turbo fetches a link when the pointer reaches it. Off, so that a
         request seen by the server is a page somebody actually visited. -->
    <meta name="turbo-prefetch" content="false">
    <script>
      // What a page's own script finds when it runs, which is what a Stimulus
      // controller finds in connect().
      window.__bridgeWhenThePageRan = Boolean(window.TurboDesktop && window.TurboDesktop.isNative);
      document.addEventListener("turbo-desktop:ready", () => { window.__heardReady = true; });
    </script>
    <script src="/turbo.js"></script>
  </head>
  <body>
    <h1 id="heading">${title}</h1>
    <nav>
      <a id="to-home" href="/">Home</a>
      <a id="to-tasks" href="/tasks">Tasks</a>
      <a id="to-tasks-replacing" href="/tasks?page=2" data-turbo-action="replace">Page 2</a>
      <a id="to-new-task" href="/tasks/new">New task</a>
      <a id="to-edit-task" href="/tasks/1/edit">Edit task</a>
      <a id="to-report" href="/reports/1">Report</a>
      <a id="to-declared" href="/declared">Declared</a>
      <a id="to-dashboard" href="/dashboard">Dashboard</a>
      <a id="to-component" href="/handled-by-a-component">Handled by a component</a>
      <a id="to-broken" href="/boom">Broken</a>
      <a id="to-elsewhere" href="https://example.com/">Elsewhere</a>
    </nav>
    ${body}
  </body>
</html>`;
}

const PAGES = {
  "/": () => page("Home", "<p>Home</p>"),
  "/tasks": () => page("Tasks", "<ul><li>One</li><li>Two</li></ul>"),
  "/tasks/new": () => page("New task", '<form id="form"><input name="title"></form>'),
  "/tasks/1/edit": () => page("Edit task", '<form id="form"><input name="title" value="One"></form>'),
  "/reports/1": () => page("Report", "<p>Figures</p>"),
  // Components declared in the markup, the way the Rails helper writes them.
  "/declared": () =>
    page(
      "Declared",
      `<button id="export"
               data-turbo-desktop-bridge="menu-item"
               data-turbo-desktop-bridge-title="Export PDF"
               data-turbo-desktop-bridge-shortcut="CmdOrCtrl+E"
               onclick="window.__exported = (window.__exported || 0) + 1">Export PDF</button>`
    ),
  "/dashboard": () => page("Dashboard", "<p>Overview</p>"),
  "/handled-by-a-component": () => page("Not shown", "<p>The shell should never load this.</p>"),
};

function respond(res, status, type, body) {
  res.writeHead(status, { "content-type": type, "cache-control": "no-store" });
  res.end(body);
}

function handle(req, res) {
  const url = new URL(req.url, `http://localhost:${port}`);
  const path = url.pathname;

  if (path === "/__requests") {
    return respond(res, 200, "application/json", JSON.stringify(requests));
  }

  requests.push({ path, userAgent: req.headers["user-agent"] || "" });

  // Go away for a while, then come back: the server restarting under the app.
  if (path === "/__outage") {
    const seconds = Number(url.searchParams.get("seconds") || 8);
    respond(res, 200, "text/plain", `down for ${seconds}s`);
    setTimeout(() => {
      server.close();
      server.closeAllConnections();
      setTimeout(() => server.listen(port, "127.0.0.1"), seconds * 1000);
    }, 100);
    return;
  }

  if (path === "/up") return respond(res, 200, "text/plain", "ok");
  if (path === "/turbo.js") return respond(res, 200, "text/javascript", TURBO);
  if (path === "/boom") return respond(res, 500, "text/html", page("Broken", "<p>500</p>"));

  if (path === "/turbo-desktop/path-configuration.json") {
    return respond(res, 200, "application/json", JSON.stringify(RULES));
  }
  if (path === "/turbo-desktop/inspector.js") {
    return respond(res, 200, "text/javascript", readFileSync(join(repoRoot, "src", "inspector.js")));
  }
  if (path.startsWith("/turbo-desktop/inspector/")) {
    const name = path.slice("/turbo-desktop/inspector/".length);
    if (!INSPECTOR_MODULES.includes(name)) return respond(res, 404, "text/plain", "not found");
    return respond(res, 200, "text/javascript", readFileSync(join(repoRoot, "src", "inspector", name)));
  }

  const render = PAGES[path];
  if (render) return respond(res, 200, "text/html", render());

  respond(res, 404, "text/html", page("Not found", "<p>404</p>"));
}

const server = createServer(handle);
server.listen(port, "127.0.0.1", () => {
  console.log(`e2e app listening on http://127.0.0.1:${port}`);
});
