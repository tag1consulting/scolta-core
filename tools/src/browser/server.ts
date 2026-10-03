// Static server for the browser tests and the benchmark.
//
// Serves each artifact from its EXTRACTED RELEASE TARBALL, never from the
// build directory, so a member the packager drops fails to load here. The
// tarballs come from SCOLTA_CORE_TARBALLS (comma-separated; CI passes the
// ones it is about to validate) or, when unset, from packing the current
// build directories with scripts/package-release.sh. A third source, for
// benchmarking builds that were never packed, is SCOLTA_CORE_DIRS: a JSON
// object of route name to directory.
//
// Every response carries the strict CSP a site embedding Scolta can use:
// scripts only from the origin, WebAssembly compilation allowed, nothing
// inline. A page that needs anything more fails the CSP test.

import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, statSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { extname, join, normalize, resolve } from "node:path";
import { extractTarball, identifyDir } from "../artifacts.js";

export const CSP =
  "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; worker-src 'self'; base-uri 'none'; form-action 'none'";

const ROOT = resolve(import.meta.dirname, "../../..");
const PAGE_DIR = resolve(import.meta.dirname, "page");
const FIXTURE_DIR = join(ROOT, "tests", "fixtures");

const TYPES: Record<string, string> = {
  ".js": "text/javascript; charset=utf-8",
  ".wasm": "application/wasm",
  ".json": "application/json; charset=utf-8",
  ".html": "text/html; charset=utf-8",
  ".ts": "text/plain; charset=utf-8",
};

/** Route name (`full`, `slim`, or a benchmark label) to served directory. */
function artifactDirs(): Map<string, string> {
  const dirs = new Map<string, string>();
  const named = process.env["SCOLTA_CORE_DIRS"];
  if (named) {
    for (const [route, dir] of Object.entries(JSON.parse(named) as Record<string, string>)) {
      dirs.set(route, resolve(dir));
    }
    return dirs;
  }
  let tarballs = (process.env["SCOLTA_CORE_TARBALLS"] ?? "").split(",").filter((t) => t !== "");
  if (tarballs.length === 0) {
    const out = mkdtempSync(join(tmpdir(), "scolta-core-tarballs-"));
    tarballs = execFileSync(join(ROOT, "scripts", "package-release.sh"), ["test", out], { encoding: "utf8" })
      .trim()
      .split("\n");
  }
  for (const tarball of tarballs) {
    const dir = extractTarball(tarball);
    dirs.set(identifyDir(dir).name, dir);
  }
  return dirs;
}

const PAGE = `<!doctype html>
<meta charset="utf-8">
<title>scolta-core harness</title>
<script type="module" src="/harness/harness.js"></script>
`;

export function serve(port: number): void {
  const dirs = artifactDirs();
  const routes: Record<string, string> = Object.fromEntries(dirs);
  createServer((req, res) => {
    const url = new URL(req.url ?? "/", "http://localhost");
    const send = (status: number, type: string, body: string | Buffer) => {
      res.writeHead(status, {
        "content-type": type,
        "content-security-policy": CSP,
        "cache-control": "no-store",
      });
      res.end(body);
    };
    const file = (base: string, rel: string) => {
      const path = normalize(join(base, rel));
      if (!path.startsWith(base) || !existsSync(path) || !statSync(path).isFile()) {
        send(404, "text/plain", "not found");
        return;
      }
      send(200, TYPES[extname(path)] ?? "application/octet-stream", readFileSync(path));
    };
    const [, top, name, ...rest] = url.pathname.split("/");
    if (url.pathname === "/" || url.pathname === "/index.html") {
      send(200, TYPES[".html"] ?? "text/html", PAGE);
    } else if (url.pathname === "/routes.json") {
      send(200, TYPES[".json"] ?? "application/json", JSON.stringify(routes));
    } else if (top === "artifact" && name !== undefined && dirs.has(name)) {
      file(dirs.get(name) ?? "", rest.join("/"));
    } else if (top === "harness") {
      file(PAGE_DIR, [name, ...rest].join("/"));
    } else if (top === "fixtures") {
      file(FIXTURE_DIR, [name, ...rest].join("/"));
    } else {
      send(404, "text/plain", "not found");
    }
  }).listen(port, "127.0.0.1", () => {
    process.stderr.write(`serving ${[...dirs.keys()].join(", ")} on http://127.0.0.1:${port}\n`);
  });
}

if (import.meta.url === `file://${process.argv[1]}`) {
  serve(Number(process.env["PORT"] ?? 4174));
}
