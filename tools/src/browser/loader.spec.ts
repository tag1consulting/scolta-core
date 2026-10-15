// The slim artifact's loader, from the extracted release tarball, in each
// engine, under the strict CSP the server sends: the .gz served as a plain
// file, the same file served with Content-Encoding: gzip, the raw .wasm, and
// files it must refuse.

import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
import { ARTIFACTS } from "../artifacts.js";
import { AI_EXPORTS, FIXED_NOW_MS, argument } from "../fixture-inputs.js";
import type { LoadResult } from "./page/harness.js";

const FIXTURES = resolve(import.meta.dirname, "../../../tests/fixtures");
const STEM = ARTIFACTS.slim.stem;
const BASE = `/artifact/slim/${STEM}`;

interface ParityFixture {
  cases: { fn: string; input: unknown; output?: unknown; error?: string }[];
}
const parity = JSON.parse(readFileSync(`${FIXTURES}/search-parity.json`, "utf8")) as ParityFixture;
const searchCases = parity.cases.filter((c) => !AI_EXPORTS.includes(c.fn));

function parse(text: string): unknown {
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return text;
  }
}

async function open(page: Page): Promise<void> {
  await page.addInitScript((now) => {
    Date.now = () => now;
  }, FIXED_NOW_MS);
  await page.goto("/");
  await page.waitForFunction(() => window.scolta !== undefined);
}

function loadVia(page: Page, url?: string): Promise<LoadResult> {
  return page.evaluate(({ stem, url }) => window.scolta.loadVia("slim", stem, url), { stem: STEM, url });
}

/** The loaded module is slim and reproduces main's search outputs. */
async function expectWorkingSlim(page: Page): Promise<void> {
  const describe = await page.evaluate(() => window.scolta.call("describe", ""));
  expect((JSON.parse(describe.output ?? "null") as { artifact: string }).artifact).toBe("slim");
  const results = await page.evaluate(
    (list) => list.map(({ fn, arg }) => window.scolta.call(fn, arg)),
    searchCases.map((c) => ({ fn: c.fn, arg: argument(c.input) })),
  );
  searchCases.forEach((c, i) => {
    const r = results[i] ?? {};
    const output = r.output === undefined ? undefined : parse(r.output);
    expect(r.error, `case ${i} ${c.fn}`).toBe(c.error);
    expect(isDeepStrictEqual(output, c.output), `case ${i} ${c.fn}`).toBe(true);
  });
  expect(await page.evaluate(() => window.scolta.violations)).toEqual([]);
}

test.describe("slim loader", () => {
  test.beforeEach(async ({ page }) => {
    await open(page);
  });

  test("inflates the .gz a server sends without Content-Encoding", async ({ page }) => {
    const response = page.waitForResponse((r) => r.url().endsWith(`${STEM}_bg.wasm.gz`));
    const result = await loadVia(page);
    expect(result.error).toBeUndefined();
    const headers = (await response).headers();
    expect(headers["content-encoding"]).toBeUndefined();
    expect(headers["content-type"]).toBe("application/gzip");
    await expectWorkingSlim(page);
  });

  test("uses the module as it is when the server sent the .gz with Content-Encoding: gzip", async ({ page }) => {
    const response = page.waitForResponse((r) => r.url().includes("encoding=gzip"));
    const result = await loadVia(page, `${BASE}_bg.wasm.gz?encoding=gzip`);
    expect(result.error).toBeUndefined();
    expect((await response).headers()["content-encoding"]).toBe("gzip");
    await expectWorkingSlim(page);
  });

  test("accepts the raw .wasm", async ({ page }) => {
    const result = await loadVia(page, `${BASE}_bg.wasm`);
    expect(result.error).toBeUndefined();
    await expectWorkingSlim(page);
  });

  test("refuses a file that is neither, naming the URL and its first bytes", async ({ page }) => {
    const result = await loadVia(page, "/bytes/page.html");
    expect(result.error).toContain("/bytes/page.html");
    expect(result.error).toContain("neither a WebAssembly module nor gzip");
    expect(result.error).toContain("first bytes: 3c 21 64 6f 63 74 79 70");
  });

  test("refuses a corrupt gzip stream, then loads on retry", async ({ page }) => {
    const corrupt = await loadVia(page, "/bytes/corrupt.gz");
    expect(corrupt.error).toContain("/bytes/corrupt.gz");
    expect(corrupt.error).toContain("not a valid gzip stream");
    const missing = await loadVia(page, `${BASE}_bg.wasm.gz.missing`);
    expect(missing.error).toContain("HTTP 404");
    // A failure leaves nothing initialized and nothing cached.
    const retry = await loadVia(page);
    expect(retry.error).toBeUndefined();
    await expectWorkingSlim(page);
  });
});
