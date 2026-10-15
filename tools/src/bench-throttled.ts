// npm run bench:throttled -- LABEL=DIR [LABEL=DIR]... [--cold N] [--warm N] [--cpu 4,6] [--profiles fast4g,slow4g]
//
// Chromium only, throttled through the DevTools protocol: CPU slowed by each
// --cpu factor and the network emulated at each profile below, cache off.
// For every build, and for a build that ships a pre-gzipped module also its
// loader path (`LABEL+loader`), it reports:
//
//   cold    download, compile and initialize, in a fresh context each sample
//   warm    score_results and batch_score_results after a warm-up, each
//           sample the mean of 20 calls, under the CPU factor alone
//
// as the median and p95 in milliseconds. The test server sends every file
// as it is, with no Content-Encoding: the raw .wasm crosses the emulated
// network at its raw size, which is what a server that does not compress
// application/wasm does.
//
// This is desktop Chromium with emulated CPU and network on this machine,
// not a phone: it shows how the builds compare when the CPU and the network
// are slow, not what any device measures.

import { chromium, type CDPSession } from "@playwright/test";
import { resolve } from "node:path";
import { type Stats, coldLoad, stats, variants } from "./bench.js";
import { FIXED_NOW_MS, argument, parityCases } from "./fixture-inputs.js";

export interface NetworkProfile {
  description: string;
  /** Added to each request, in ms. */
  latencyMs: number;
  downloadKbps: number;
  uploadKbps: number;
}

export const PROFILES: Readonly<Record<string, NetworkProfile>> = {
  fast4g: { description: "fast 4G", latencyMs: 60, downloadKbps: 9_000, uploadKbps: 1_500 },
  slow4g: { description: "slow 4G", latencyMs: 150, downloadKbps: 1_600, uploadKbps: 750 },
};

async function throttle(cdp: CDPSession, cpu: number, profile: NetworkProfile | null): Promise<void> {
  await cdp.send("Emulation.setCPUThrottlingRate", { rate: cpu });
  await cdp.send("Network.enable");
  await cdp.send("Network.setCacheDisabled", { cacheDisabled: true });
  await cdp.send("Network.emulateNetworkConditions", {
    offline: false,
    latency: profile?.latencyMs ?? 0,
    // Bytes per second; -1 turns throttling off.
    downloadThroughput: profile === null ? -1 : (profile.downloadKbps * 1000) / 8,
    uploadThroughput: profile === null ? -1 : (profile.uploadKbps * 1000) / 8,
  });
}

/** The scoring inputs: the fixture's heaviest case, a 100-result query, and the batch. */
function scoringInputs(): Record<string, { fn: string; arg: string; results: number }> {
  const cases = parityCases();
  const scores = cases
    .filter((c) => c.fn === "score_results")
    .map((c) => c.input as { query: string; results: unknown[]; config: unknown });
  const heaviest = [...scores].sort((a, b) => argument(b).length - argument(a).length)[0];
  const batch = cases.find((c) => c.fn === "batch_score_results")?.input as { queries: unknown[] } | undefined;
  if (heaviest === undefined || batch === undefined) throw new Error("fixture inputs lack scoring cases");
  const pool = scores.flatMap((s) => s.results);
  const hundred = { ...heaviest, results: Array.from({ length: 100 }, (_, i) => pool[i % pool.length]) };
  const batchResults = (batch.queries as { results: unknown[] }[]).reduce((n, q) => n + q.results.length, 0);
  return {
    score_results: { fn: "score_results", arg: argument(heaviest), results: heaviest.results.length },
    score_results_100: { fn: "score_results", arg: argument(hundred), results: 100 },
    batch_score_results: { fn: "batch_score_results", arg: argument(batch), results: batchResults },
  };
}

async function main(argv: readonly string[]): Promise<void> {
  const builds: Record<string, string> = {};
  let cold = 7;
  let warm = 30;
  let cpus = [4, 6];
  let profiles = Object.keys(PROFILES);
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i] ?? "";
    if (arg === "--cold") cold = Number(argv[++i]);
    else if (arg === "--warm") warm = Number(argv[++i]);
    else if (arg === "--cpu") cpus = (argv[++i] ?? "").split(",").map(Number);
    else if (arg === "--profiles") profiles = (argv[++i] ?? "").split(",");
    else {
      const [label, dir] = arg.split("=");
      if (!label || !dir) throw new Error(`expected LABEL=DIR, got ${arg}`);
      builds[label] = resolve(dir);
    }
  }
  if (Object.keys(builds).length === 0) throw new Error("name at least one LABEL=DIR build");
  for (const name of profiles) if (!(name in PROFILES)) throw new Error(`unknown profile ${name}`);

  process.env["SCOLTA_CORE_DIRS"] = JSON.stringify(builds);
  const port = 4176;
  const { serve } = await import("./browser/server.js");
  serve(port);
  const inputs = scoringInputs();

  const browser = await chromium.launch();
  const chromiumVersion = browser.version();
  const cold_ms: Record<string, Record<string, Record<string, Stats>>> = {};
  const warm_ms: Record<string, Record<string, Record<string, Stats>>> = {};
  const fresh = async (cpu: number, profile: NetworkProfile | null) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    await page.addInitScript((now) => {
      Date.now = () => now;
    }, FIXED_NOW_MS);
    await page.goto(`http://127.0.0.1:${port}/`);
    await page.waitForFunction(() => window.scolta !== undefined);
    // After the harness page is up: only the module's own fetches are timed.
    await throttle(await context.newCDPSession(page), cpu, profile);
    return { context, page };
  };
  for (const cpu of cpus) {
    for (const variant of variants(builds)) {
      for (const name of profiles) {
        const profile = PROFILES[name] ?? null;
        const samples: number[] = [];
        for (let i = 0; i < cold; i++) {
          const { context, page } = await fresh(cpu, profile);
          samples.push(await coldLoad(page, variant.label, variant.stem, variant.loader));
          await context.close();
        }
        ((cold_ms[`cpu${cpu}x`] ??= {})[name] ??= {})[variant.name] = stats(samples);
        process.stderr.write(`cold cpu ${cpu}x ${name} ${variant.name} done\n`);
      }
      // Warm scoring depends on the CPU, not on how the module arrived, so
      // it is measured once per build, through its glue.
      if (variant.loader) continue;
      const { context, page } = await fresh(cpu, null);
      await coldLoad(page, variant.label, variant.stem, false);
      const timings: Record<string, Stats> = {};
      for (const [key, { fn, arg }] of Object.entries(inputs)) {
        await page.evaluate(({ fn, arg }) => window.scolta.time(fn, arg, 20), { fn, arg });
        timings[key] = stats(
          await page.evaluate(({ fn, arg, n }) => window.scolta.time(fn, arg, n, 20), { fn, arg, n: warm }),
        );
      }
      (warm_ms[`cpu${cpu}x`] ??= {})[variant.name] = timings;
      await context.close();
      process.stderr.write(`warm cpu ${cpu}x ${variant.name} done\n`);
    }
  }
  await browser.close();
  process.stdout.write(
    `${JSON.stringify(
      {
        tool: "scolta-core bench:throttled",
        note: "desktop Chromium with CDP CPU throttling and network emulation, not a device; files served without Content-Encoding",
        chromium: chromiumVersion,
        profiles: Object.fromEntries(profiles.map((name) => [name, PROFILES[name]])),
        cpu_factors: cpus,
        samples: { cold, warm, calls_per_warm_sample: 20 },
        inputs: Object.fromEntries(Object.entries(inputs).map(([k, v]) => [k, { fn: v.fn, results: v.results }])),
        builds,
        cold_ms,
        warm_ms,
      },
      null,
      2,
    )}\n`,
  );
  process.exit(0);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  await main(process.argv.slice(2));
}
