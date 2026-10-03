// npm run measure:size -- [--tarball FILE]... [--dir DIR]... [--budgets FILE] [--no-budget]
//
// Prints a machine-readable matrix of raw, gzip -9 and Brotli 11 bytes for
// every member of each artifact and the sum of what a browser downloads (the
// module and its glue), then fails if any size is over its budget in
// size-budgets.json. With no --tarball or --dir it measures the build
// directories scripts/build.sh writes. gzip is a pinned pure-JS deflate and
// Brotli is the one bundled with Node, both with fixed parameters and no
// timestamp, so the numbers reproduce on any machine; they are laboratory
// sizes, not what any server sends.

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { gzipSync } from "fflate";
import { constants, brotliCompressSync } from "node:zlib";
import {
  ARTIFACTS,
  type ArtifactName,
  type ArtifactSpec,
  extractTarball,
  identifyDir,
  members,
  servedMembers,
} from "./artifacts.js";

export interface Sizes {
  raw: number;
  gzip: number;
  brotli: number;
}

const SIZE_KEYS = ["raw", "gzip", "brotli"] as const;

export function measureBytes(bytes: Uint8Array): Sizes {
  return {
    raw: bytes.byteLength,
    // fflate, not node:zlib: Node links whichever zlib its build chose (the
    // system one, or Chromium's fork, whose output also varies by CPU), and
    // those differ by about 2% at level 9. A pinned pure-JS deflate gives
    // the same bytes everywhere. Brotli is bundled with Node, the same 1.2.0
    // on every supported version, with no CPU-dependent output.
    gzip: gzipSync(bytes, { level: 9, mem: 12, mtime: 0 }).byteLength,
    brotli: brotliCompressSync(bytes, {
      params: {
        [constants.BROTLI_PARAM_QUALITY]: 11,
        [constants.BROTLI_PARAM_LGWIN]: 22,
        [constants.BROTLI_PARAM_SIZE_HINT]: bytes.byteLength,
      },
    }).byteLength,
  };
}

function sum(all: readonly Sizes[]): Sizes {
  return all.reduce((a, b) => ({ raw: a.raw + b.raw, gzip: a.gzip + b.gzip, brotli: a.brotli + b.brotli }), {
    raw: 0,
    gzip: 0,
    brotli: 0,
  });
}

interface ArtifactReport {
  artifact: ArtifactName;
  source: string;
  files: Record<string, Sizes & { served: boolean }>;
  served_total: Sizes;
  budget: Record<string, Sizes> | null;
  breaches: string[];
}

type Budgets = Partial<Record<ArtifactName, Record<string, Sizes>>>;

function measureArtifact(spec: ArtifactSpec, dir: string, source: string, budgets: Budgets | null): ArtifactReport {
  const files: ArtifactReport["files"] = {};
  const breaches: string[] = [];
  const served = servedMembers(spec);
  for (const name of members(spec)) {
    const path = join(dir, name);
    if (!existsSync(path)) {
      breaches.push(`${spec.name}: ${name} is missing from ${source}`);
      continue;
    }
    files[name] = { ...measureBytes(readFileSync(path)), served: served.includes(name) };
  }
  const servedTotal = sum(served.flatMap((name) => (files[name] ? [files[name]] : [])));
  const budget = budgets?.[spec.name] ?? null;
  if (budgets !== null && budget === null) {
    breaches.push(`${spec.name}: no budget in size-budgets.json`);
  }
  if (budget !== null) {
    for (const [name, limit] of Object.entries(budget)) {
      const actual = name === "served_total" ? servedTotal : files[name];
      if (actual === undefined) {
        breaches.push(`${spec.name}: budget names ${name}, which the artifact does not have`);
        continue;
      }
      for (const key of SIZE_KEYS) {
        if (actual[key] > limit[key]) {
          breaches.push(`${spec.name}: ${name} ${key} is ${actual[key]} bytes, over its ${limit[key]}-byte budget`);
        }
      }
    }
  }
  return { artifact: spec.name, source, files, served_total: servedTotal, budget, breaches };
}

function main(argv: readonly string[]): number {
  const tarballs: string[] = [];
  const dirs: string[] = [];
  let budgetsPath = "size-budgets.json";
  let enforce = true;
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    const value = (): string => {
      const next = argv[++i];
      if (next === undefined) throw new Error(`${arg} needs a value`);
      return next;
    };
    if (arg === "--tarball") tarballs.push(value());
    else if (arg === "--dir") dirs.push(value());
    else if (arg === "--budgets") budgetsPath = value();
    else if (arg === "--no-budget") enforce = false;
    else throw new Error(`unknown argument: ${arg}`);
  }
  if (tarballs.length === 0 && dirs.length === 0) {
    dirs.push(...Object.values(ARTIFACTS).map((spec) => spec.buildDir));
  }
  const budgets: Budgets | null = enforce
    ? (JSON.parse(readFileSync(budgetsPath, "utf8")) as { artifacts: Budgets }).artifacts
    : null;

  const reports: ArtifactReport[] = [];
  for (const tarball of tarballs) {
    const dir = extractTarball(tarball);
    reports.push(measureArtifact(identifyDir(dir), dir, tarball, budgets));
  }
  for (const dir of dirs) {
    reports.push(measureArtifact(identifyDir(dir), dir, dir, budgets));
  }

  const breaches = reports.flatMap((r) => r.breaches);
  const output = {
    tool: "scolta-core measure:size",
    node: process.version,
    brotli: process.versions.brotli,
    compression: { gzip: "fflate 0.8.3 level 9", brotli: "node:zlib brotli quality 11, lgwin 22" },
    artifacts: reports,
    pass: breaches.length === 0,
  };
  process.stdout.write(`${JSON.stringify(output, null, 2)}\n`);
  for (const breach of breaches) {
    process.stderr.write(`FAIL: ${breach}\n`);
  }
  return breaches.length === 0 ? 0 : 1;
}

if (import.meta.url === `file://${process.argv[1]}`) {
  process.exitCode = main(process.argv.slice(2));
}
