// Both artifacts, loaded from their extracted release tarballs in each
// engine, under the strict CSP the server sends.

import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
import { ARTIFACTS, type ArtifactName } from "../artifacts.js";
import { AI_EXPORTS, FIXED_NOW_MS, argument } from "../fixture-inputs.js";
import type { CallResult } from "./page/harness.js";

const FIXTURES = resolve(import.meta.dirname, "../../../tests/fixtures");

interface ParityFixture {
  cases: { fn: string; input: unknown; output?: unknown; error?: string }[];
}
interface SanitizeFixture {
  cases: { query: string; config: Record<string, boolean>; expected: string }[];
}

const parity = JSON.parse(readFileSync(`${FIXTURES}/search-parity.json`, "utf8")) as ParityFixture;
const sanitize = JSON.parse(readFileSync(`${FIXTURES}/sanitize-differential.json`, "utf8")) as SanitizeFixture;

function parse(text: string): unknown {
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return text;
  }
}

async function open(page: Page, name: ArtifactName): Promise<void> {
  await page.addInitScript((now) => {
    Date.now = () => now;
  }, FIXED_NOW_MS);
  await page.goto("/");
  await page.waitForFunction(() => window.scolta !== undefined);
  await page.evaluate(({ route, stem }) => window.scolta.load(route, stem), {
    route: name,
    stem: ARTIFACTS[name].stem,
  });
}

async function callAll(page: Page, calls: { fn: string; arg: string }[]): Promise<CallResult[]> {
  return page.evaluate((list) => list.map(({ fn, arg }) => window.scolta.call(fn, arg)), calls);
}

for (const name of ["full", "slim"] as const) {
  test.describe(`${name} artifact`, () => {
    test.beforeEach(async ({ page }) => {
      await open(page, name);
    });

    test("loads under the strict CSP and identifies itself", async ({ page }) => {
      const describe = await page.evaluate(() => window.scolta.call("describe", ""));
      const manifest = JSON.parse(describe.output ?? "null") as {
        artifact: string;
        capabilities: { custom_patterns: boolean; ai_exports: boolean };
        functions: Record<string, unknown>;
      };
      expect(manifest.artifact).toBe(name);
      expect(manifest.capabilities).toEqual({ custom_patterns: name === "full", ai_exports: name === "full" });
      const exported = await page.evaluate(() => window.scolta.exports());
      for (const fn of Object.keys(manifest.functions)) {
        expect(exported, `describe() lists ${fn}`).toContain(fn);
      }
      for (const fn of AI_EXPORTS) {
        expect(exported.includes(fn), fn).toBe(name === "full");
      }
      expect(await page.evaluate(() => window.scolta.violations)).toEqual([]);
    });

    test("reproduces the search outputs captured from main", async ({ page }) => {
      const results = await callAll(
        page,
        parity.cases.map((c) => ({ fn: c.fn, arg: argument(c.input) })),
      );
      expect(results.length).toBe(parity.cases.length);
      parity.cases.forEach((c, i) => {
        const r = results[i] ?? {};
        if (name === "slim" && AI_EXPORTS.includes(c.fn)) {
          expect(r.missing, `${c.fn} must be absent from slim`).toBe(true);
          return;
        }
        const label = `case ${i} ${c.fn}`;
        expect(r.error, label).toBe(c.error);
        expect(isDeepStrictEqual(r.output === undefined ? undefined : parse(r.output), c.output), label).toBe(true);
      });
    });

    test("redacts exactly as the regex implementation did", async ({ page }) => {
      const results = await callAll(
        page,
        sanitize.cases.map((c) => ({ fn: "sanitize_query", arg: JSON.stringify({ query: c.query, config: c.config }) })),
      );
      sanitize.cases.forEach((c, i) => {
        expect(results[i]?.error, c.query).toBeUndefined();
        expect(JSON.parse(results[i]?.output ?? "null"), JSON.stringify(c.query)).toBe(c.expected);
      });
    });

    test("never returns a query it could not sanitize", async ({ page }) => {
      const results = await callAll(page, [
        // A lone surrogate survives JSON.stringify as an escape that the
        // module's JSON parser rejects.
        { fn: "sanitize_query", arg: JSON.stringify({ query: "ssn 123-45-6789 \ud800" }) },
        { fn: "sanitize_query", arg: "not json" },
        { fn: "sanitize_query", arg: JSON.stringify({ config: {} }) },
        { fn: "sanitize_query", arg: JSON.stringify({ query: 42 }) },
      ]);
      for (const r of results) {
        expect(r.output).toBeUndefined();
        expect(r.error).toBeTruthy();
      }
    });

    test("handles custom patterns as its capabilities say", async ({ page }) => {
      const run = (patterns: unknown) =>
        page.evaluate(
          (arg) => window.scolta.call("sanitize_query", arg),
          JSON.stringify({ query: "patient MRN-12345 seen", config: { custom_patterns: patterns } }),
        );
      const capture = await run([{ regex: "MRN-(\\d+)", replacement: "MRN-[$1]" }]);
      const invalid = await run([{ regex: "MRN-(\\d+", replacement: "x" }]);
      const empty = await run([]);
      expect(JSON.parse(empty.output ?? "null")).toBe("patient MRN-12345 seen");
      if (name === "full") {
        expect(JSON.parse(capture.output ?? "null")).toBe("patient MRN-[12345] seen");
        expect(invalid.error).toContain("invalid regex");
      } else {
        // Never silently ignored: the call fails rather than returning the
        // query with the pattern unapplied.
        expect(capture.output).toBeUndefined();
        expect(capture.error).toContain("not supported");
        expect(invalid.output).toBeUndefined();
      }
    });

    test("finishes hostile inputs inside a worker deadline", async ({ page }) => {
      const hostile = [
        "aaaaa:".repeat(20_000),
        "1".repeat(120_000),
        "1-".repeat(60_000),
        "١".repeat(60_000),
        `${"a".repeat(60_000)}@${"b-".repeat(30_000)}`,
        "a@".repeat(60_000),
        "+1 (555) 867-530 ".repeat(7_000),
      ];
      for (const query of hostile) {
        const result = await page.evaluate(
          ({ route, stem, arg }) => window.scolta.sanitizeWithDeadline(route, stem, arg, 5_000),
          { route: name, stem: ARTIFACTS[name].stem, arg: JSON.stringify({ query }) },
        );
        expect(result.status, `${query.slice(0, 12)}... took ${Math.round(result.ms)} ms`).toBe("ok");
      }
      if (name === "full") {
        const result = await page.evaluate(
          ({ route, stem, arg }) => window.scolta.sanitizeWithDeadline(route, stem, arg, 5_000),
          {
            route: name,
            stem: ARTIFACTS[name].stem,
            arg: JSON.stringify({
              query: `${"a".repeat(50_000)}!`,
              config: { custom_patterns: [{ regex: "(a+)+$", replacement: "[X]" }] },
            }),
          },
        );
        expect(result.status).toBe("ok");
      }
    });
  });
}
