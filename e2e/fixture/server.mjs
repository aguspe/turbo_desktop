// The app server the shell points at during e2e runs. Static and
// dependency-free: the tests exercise the bridge, not an app.
import { createServer } from "node:http";

const PAGE = `<!doctype html>
<html>
  <head><title>E2E Fixture</title></head>
  <body>
    <h1 id="heading">Turbo Desktop E2E fixture</h1>
  </body>
</html>`;

export function startFixtureServer(port = 3210) {
  const server = createServer((req, res) => {
    res.writeHead(200, { "content-type": "text/html" });
    res.end(PAGE);
  });

  return new Promise((resolve) => {
    server.listen(port, "127.0.0.1", () => resolve(server));
  });
}
