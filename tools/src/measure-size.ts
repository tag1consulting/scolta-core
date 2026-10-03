// npm run measure:size -- [--tarball FILE]... [--dir DIR]... [--budgets FILE] [--no-budget] [--artifacts full,slim]
//
// Prints a machine-readable matrix of raw, gzip -9 and Brotli 11 bytes for
// every member of each artifact and the sum of what a browser downloads (the
// module and its glue; for the slim artifact, also the pre-gzipped module,
// its loader and the glue), then fails if any size is over its budget in
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
  servedPregzippedMembers,
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
  /**
   * Through the loader: the .gz counts at its raw size under every key,
   * because it is already compressed and crosses the wire as it is, and the
   * loader and glue count as JavaScript a server may compress. Absent for
   * an artifact that is not pre-gzipped.
   */
  served_pregzipped_total?: Sizes;
  budget: Record<string, Partial<Sizes>> | null;
  breaches: string[];
}

/** A budget may leave a key out: a .gz has only a meaningful raw size. */
type Budgets = Partial<Record<ArtifactName, Record<string, Partial<Sizes>>>>;

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
  const pregzipped = servedPregzippedMembers(spec);
  const pregzippedTotal =
    pregzipped.length === 0
      ? undefined
      : sum(
          pregzipped.flatMap((name) => {
            const sizes = files[name];
            if (sizes === undefined) return [];
            return [name.endsWith(".gz") ? { raw: sizes.raw, gzip: sizes.raw, brotli: sizes.raw } : sizes];
          }),
        );
  const totals: Record<string, Sizes | undefined> = {
    served_total: servedTotal,
    served_pregzipped_total: pregzippedTotal,
  };
  const budget = budgets?.[spec.name] ?? null;
  if (budgets !== null && budget === null) {
    breaches.push(`${spec.name}: no budget in size-budgets.json`);
  }
  if (budget !== null) {
    for (const [name, limit] of Object.entries(budget)) {
      const actual = name in totals ? totals[name] : files[name];
      if (actual === undefined) {
        breaches.push(`${spec.name}: budget names ${name}, which the artifact does not have`);
        continue;
      }
      const keys = SIZE_KEYS.filter((key) => limit[key] !== undefined);
      if (keys.length === 0) {
        breaches.push(`${spec.name}: budget for ${name} sets no raw, gzip or brotli limit`);
      }
      for (const key of keys) {
        const max = limit[key] ?? 0;
        if (actual[key] > max) {
          breaches.push(`${spec.name}: ${name} ${key} is ${actual[key]} bytes, over its ${max}-byte budget`);
        }
      }
    }
  }
  return {
    artifact: spec.name,
    source,
    files,
    served_total: servedTotal,
    ...(pregzippedTotal === undefined ? {} : { served_pregzipped_total: pregzippedTotal }),
    budget,
    breaches,
  };
}

function main(argv: readonly string[]): number {
  const tarballs: string[] = [];
  const dirs: string[] = [];
  let budgetsPath = "size-budgets.json";
  let enforce = true;
  let required: string[] | undefined;
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
    else if (arg === "--artifacts")
      required = value()
        .split(",")
        .map((name) => name.trim())
        .filter((name) => name !== "");
    else throw new Error(`unknown argument: ${arg}`);
  }
  if (tarballs.length === 0 && dirs.length === 0) {
    dirs.push(...Object.values(ARTIFACTS).map((spec) => spec.buildDir));
  }
  const budgets: Budgets | null = enforce
    ? (JSON.parse(readFileSync(budgetsPath, "utf8")) as { artifacts: Budgets }).artifacts
    : null;

  const reports: ArtifactReport[] = [];
  // A source that cannot be read or identified is a breach in the report,
  // not a crash that leaves CI with no JSON to read.
  const unreadable: string[] = [];
  const measure = (source: string, dir: () => string) => {
    try {
      const path = dir();
      reports.push(measureArtifact(identifyDir(path), path, source, budgets));
    } catch (e) {
      unreadable.push(`${source}: ${e instanceof Error ? e.message : String(e)}`);
    }
  };
  for (const tarball of tarballs) measure(tarball, () => extractTarball(tarball));
  for (const dir of dirs) measure(dir, () => dir);

  // Every budgeted artifact must have been measured, or a run handed one
  // tarball would pass without checking the other. --artifacts narrows the
  // set deliberately.
  const unknown =
    budgets === null || required === undefined
      ? []
      : required
          .filter((name) => !(name in budgets))
          .map((name) => `--artifacts names ${name}, which has no budget in size-budgets.json`);
  if (required !== undefined && required.length === 0) {
    unknown.push("--artifacts names no artifact");
  }
  const measured = new Set(reports.map((r) => r.artifact));
  const missing =
    budgets === null
      ? []
      : (required ?? Object.keys(budgets))
          .filter((name) => name in (budgets ?? {}) && !measured.has(name as ArtifactName))
          .map((name) => `${name}: not measured; pass its tarball or build directory`);

  const breaches = [...unknown, ...unreadable, ...missing, ...reports.flatMap((r) => r.breaches)];

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
