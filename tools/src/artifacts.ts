// The two published WASM artifacts, as their release tarballs ship them.
//
// scripts/build.sh produces both build directories and
// scripts/package-release.sh packs both tarballs; this file is what the
// measurement and browser tooling expects of them, and the browser tests load
// the extracted tarballs so a file the packager drops fails here rather than
// on a site.

import { execFileSync } from "node:child_process";
import { mkdtempSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

export type ArtifactName = "full" | "slim";

export interface ArtifactSpec {
  readonly name: ArtifactName;
  /** wasm-pack `--out-name`: the stem of every file in the artifact. */
  readonly stem: string;
  /** Where scripts/build.sh writes it. */
  readonly buildDir: string;
  /** Release tarball name for a version. */
  tarball(version: string): string;
}

export const ARTIFACTS: Readonly<Record<ArtifactName, ArtifactSpec>> = {
  full: {
    name: "full",
    stem: "scolta_core",
    buildDir: "pkg",
    tarball: (version) => `scolta-core-${version}.tar.gz`,
  },
  slim: {
    name: "slim",
    stem: "scolta_core_slim",
    buildDir: "pkg-slim",
    tarball: (version) => `scolta-core-slim-${version}.tar.gz`,
  },
};

/** Every member of an artifact, in tarball order. */
export function members(spec: ArtifactSpec): string[] {
  return [`${spec.stem}_bg.wasm`, `${spec.stem}.js`, `${spec.stem}.d.ts`, `${spec.stem}_bg.wasm.d.ts`];
}

/** The members a browser downloads: the module and its glue. */
export function servedMembers(spec: ArtifactSpec): string[] {
  return [`${spec.stem}_bg.wasm`, `${spec.stem}.js`];
}

/** Which artifact a set of member names belongs to, or undefined. */
export function identify(names: readonly string[]): ArtifactSpec | undefined {
  return Object.values(ARTIFACTS).find((spec) => names.includes(`${spec.stem}_bg.wasm`));
}

/** Extract a tarball into a fresh temporary directory and return its path. */
export function extractTarball(path: string): string {
  const dir = mkdtempSync(join(tmpdir(), "scolta-core-artifact-"));
  execFileSync("tar", ["-xzf", path, "-C", dir]);
  return dir;
}

/** The artifact in an extracted tarball or build directory. */
export function identifyDir(dir: string): ArtifactSpec {
  const spec = identify(readdirSync(dir));
  if (spec === undefined) {
    throw new Error(`${dir} holds neither the full nor the slim artifact`);
  }
  return spec;
}
