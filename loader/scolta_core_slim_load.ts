// Loads the slim scolta-core module from its pre-gzipped file.
//
// Many servers compress JavaScript but send application/wasm as it is, so a
// visitor downloads the raw module. The slim artifact also ships the module
// gzipped, scolta_core_slim_bg.wasm.gz, and this loader fetches it and
// inflates it in the browser with DecompressionStream. If the server already
// decoded it (it sent the file with Content-Encoding: gzip), the bytes arrive
// as a module and are used as they are. The loader tells the two apart by
// their first bytes, so it needs no configuration and also accepts the raw
// .wasm.
//
// It re-exports the glue, so it replaces scolta_core_slim.js one for one:
//
//   import init, { score_results } from "./scolta_core_slim_load.js";
//   await init();                 // fetches scolta_core_slim_bg.wasm.gz
//   await init(someOtherUrl);     // or a module or gzip file elsewhere
//
// It needs fetch and DecompressionStream, and no CSP source beyond what the
// glue needs: script-src 'self' 'wasm-unsafe-eval' and connect-src for the URL.

import glueInit, { type InitOutput } from "./scolta_core_slim.js";

export * from "./scolta_core_slim.js";

const WASM_MAGIC: readonly number[] = [0x00, 0x61, 0x73, 0x6d];
const GZIP_MAGIC: readonly number[] = [0x1f, 0x8b];

function startsWith(bytes: Uint8Array, magic: readonly number[]): boolean {
  return bytes.length >= magic.length && magic.every((b, i) => bytes[i] === b);
}

function firstBytes(bytes: Uint8Array): string {
  if (bytes.length === 0) return "none, the body is empty";
  return Array.from(bytes.subarray(0, 8), (b) => b.toString(16).padStart(2, "0")).join(" ");
}

async function gunzip(buffer: ArrayBuffer): Promise<Uint8Array> {
  const stream = new Blob([buffer]).stream().pipeThrough(new DecompressionStream("gzip"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/**
 * Fetch `url` and return the WebAssembly module in it, inflating it if it is
 * gzipped. Throws, naming the URL, on an HTTP error, a corrupt gzip stream,
 * or bytes that are neither a module nor gzip.
 */
export async function fetchModuleBytes(url: string | URL): Promise<Uint8Array> {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`scolta-core: ${String(url)} answered HTTP ${response.status}`);
  }
  const buffer = await response.arrayBuffer();
  const bytes = new Uint8Array(buffer);
  if (startsWith(bytes, WASM_MAGIC)) return bytes;
  if (!startsWith(bytes, GZIP_MAGIC)) {
    throw new Error(
      `scolta-core: ${String(url)} is neither a WebAssembly module nor gzip (first bytes: ${firstBytes(bytes)})`,
    );
  }
  let inflated: Uint8Array;
  try {
    inflated = await gunzip(buffer);
  } catch (e) {
    throw new Error(
      `scolta-core: ${String(url)} is not a valid gzip stream: ${e instanceof Error ? e.message : String(e)}`,
    );
  }
  if (!startsWith(inflated, WASM_MAGIC)) {
    throw new Error(
      `scolta-core: ${String(url)} inflates to something other than a WebAssembly module (first bytes: ${firstBytes(inflated)})`,
    );
  }
  return inflated;
}

let loading: Promise<InitOutput> | undefined;

/**
 * Fetch, inflate if needed, and initialize the module. With no URL it loads
 * scolta_core_slim_bg.wasm.gz from beside this file. Later calls return the
 * first call's result; a failed call can be retried.
 */
export default function init(
  url: string | URL = new URL("scolta_core_slim_bg.wasm.gz", import.meta.url),
): Promise<InitOutput> {
  loading ??= fetchModuleBytes(url)
    .then((bytes) => glueInit({ module_or_path: bytes }))
    .catch((e: unknown) => {
      loading = undefined;
      throw e;
    });
  return loading;
}
