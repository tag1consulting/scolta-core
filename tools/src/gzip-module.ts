// node tools/dist/gzip-module.js IN [OUT]
//
// Writes IN gzipped to OUT (default IN.gz), deterministically: zopfli at its
// default 15 iterations, compiled to WebAssembly in a pinned dev dependency,
// so the same module gives the same bytes on any machine. The gzip header
// carries no file name and a zero mtime. scripts/build.sh runs it on the
// slim module, and CI runs it twice and compares the outputs.
//
// zopfli, not fflate level 9: on the slim module it is 121,239 bytes against
// fflate's 131,936, and both are byte-identical from run to run.

import { readFileSync, writeFileSync } from "node:fs";
import { gunzipSync } from "node:zlib";
import { gzipAsync } from "@gfx/zopfli";

export const ZOPFLI_ITERATIONS = 15;

export async function gzipModule(bytes: Uint8Array): Promise<Uint8Array> {
  const gz = await gzipAsync(bytes, { numiterations: ZOPFLI_ITERATIONS, blocksplitting: true, blocksplittingmax: 15 });
  // A compressor bug must never ship: the file has to inflate back to the
  // module, byte for byte.
  if (Buffer.compare(gunzipSync(gz), bytes) !== 0) {
    throw new Error("gzip output does not inflate back to its input");
  }
  return gz;
}

async function main(argv: readonly string[]): Promise<void> {
  const [input, output = `${input}.gz`] = argv;
  if (input === undefined) throw new Error("usage: gzip-module IN [OUT]");
  const gz = await gzipModule(readFileSync(input));
  writeFileSync(output, gz);
  process.stdout.write(`${output}: ${gz.byteLength} bytes\n`);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  await main(process.argv.slice(2));
}
