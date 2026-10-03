// npm run bench:browser -- LABEL=DIR [LABEL=DIR]... [--cold N] [--warm N] [--engines a,b]
//
// Times each build in real browser engines, all with the same fixed inputs:
//
//   cold   fetch, compile and initialize the module, in a fresh context
//          each sample (nothing cached)
//   first  the first score_results call after initialization
//   warm   score_results and sanitize_query after a warm-up, each sample
//          the mean of 50 calls so a 1 ms timer can resolve it
//
// Prints JSON with the median and p95 of each. These are desktop engines on
// this machine over loopback, not a phone and not a network: compare builds
// with each other within one run, never with numbers from another machine.

import { chromium, firefox, webkit, type BrowserType } from "@playwright/test";
import { resolve } from "node:path";
import { identifyDir } from "./artifacts.js";
import { FIXED_NOW_MS, argument, parityCases } from "./fixture-inputs.js";

const ENGINES: Record<string, BrowserType> = { chromium, firefox, webkit };

interface Stats {
  median: number;
  p95: number;
  n: number;
}

function stats(samples: readonly number[]): Stats {
  const sorted = [...samples].sort((a, b) => a - b);
  const at = (q: number) => sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))] ?? NaN;
  const round = (x: number) => Math.round(x * 1000) / 1000;
  return { median: round(at(0.5)), p95: round(at(0.95)), n: sorted.length };
}

async function main(argv: readonly string[]): Promise<void> {
  const builds: Record<string, string> = {};
  let cold = 15;
  let warm = 40;
  let engines = Object.keys(ENGINES);
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i] ?? "";
    if (arg === "--cold") cold = Number(argv[++i]);
    else if (arg === "--warm") warm = Number(argv[++i]);
    else if (arg === "--engines") engines = (argv[++i] ?? "").split(",");
    else {
      const [label, dir] = arg.split("=");
      if (!label || !dir) throw new Error(`expected LABEL=DIR, got ${arg}`);
      builds[label] = resolve(dir);
    }
  }
  if (Object.keys(builds).length === 0) throw new Error("name at least one LABEL=DIR build");

  process.env["SCOLTA_CORE_DIRS"] = JSON.stringify(builds);
  const port = 4175;
  const { serve } = await import("./browser/server.js");
  serve(port);

  // The heaviest scoring case in the fixture, and a sanitizer input that
  // exercises every pattern.
  const scoreArg = argument(
    parityCases()
      .filter((c) => c.fn === "score_results")
      .map((c) => argument(c.input))
      .sort((a, b) => b.length - a.length)[0],
  );
  const sanitizeArg = JSON.stringify({
    query: "mail a.b@example.com, call +1 (555) 867-5309, ssn 123-45-6789, card 4111 1111 1111 1111, ip 10.0.0.1 or fe80::1",
  });
  // What most queries look like: nothing to redact.
  const plainArg = JSON.stringify({ query: "drupal performance tuning guide for large sites" });

  const report: Record<string, Record<string, Record<string, Stats>>> = {};
  for (const engine of engines) {
    const type = ENGINES[engine];
    if (type === undefined) throw new Error(`unknown engine ${engine}`);
    const browser = await type.launch();
    report[engine] = {};
    for (const [label, dir] of Object.entries(builds)) {
      const stem = identifyDir(dir).stem;
      const coldSamples: number[] = [];
      const firstSamples: number[] = [];
      let warmScore: number[] = [];
      let warmSanitize: number[] = [];
      let warmSanitizePlain: number[] = [];
      for (let i = 0; i < cold; i++) {
        const context = await browser.newContext();
        const page = await context.newPage();
        await page.addInitScript((now) => {
          Date.now = () => now;
        }, FIXED_NOW_MS);
        await page.goto(`http://127.0.0.1:${port}/`);
        await page.waitForFunction(() => window.scolta !== undefined);
        coldSamples.push(await page.evaluate(({ label, stem }) => window.scolta.load(label, stem), { label, stem }));
        const [first] = await page.evaluate((arg) => window.scolta.time("score_results", arg, 1), scoreArg);
        firstSamples.push(first ?? NaN);
        if (i === cold - 1) {
          await page.evaluate((arg) => window.scolta.time("score_results", arg, 50), scoreArg);
          warmScore = await page.evaluate(({ arg, n }) => window.scolta.time("score_results", arg, n, 50), {
            arg: scoreArg,
            n: warm,
          });
          await page.evaluate((arg) => window.scolta.time("sanitize_query", arg, 50), sanitizeArg);
          warmSanitize = await page.evaluate(({ arg, n }) => window.scolta.time("sanitize_query", arg, n, 50), {
            arg: sanitizeArg,
            n: warm,
          });
          warmSanitizePlain = await page.evaluate(({ arg, n }) => window.scolta.time("sanitize_query", arg, n, 50), {
            arg: plainArg,
            n: warm,
          });
        }
        await context.close();
      }
      report[engine][label] = {
        cold_init_ms: stats(coldSamples),
        first_score_ms: stats(firstSamples),
        warm_score_ms: stats(warmScore),
        warm_sanitize_ms: stats(warmSanitize),
        warm_sanitize_plain_ms: stats(warmSanitizePlain),
      };
      process.stderr.write(`${engine} ${label} done\n`);
    }
    await browser.close();
  }
  process.stdout.write(
    `${JSON.stringify({ tool: "scolta-core bench:browser", note: "desktop engines over loopback", builds, report }, null, 2)}\n`,
  );
  process.exit(0);
}

await main(process.argv.slice(2));
