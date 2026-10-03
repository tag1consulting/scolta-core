// Runs in the browser page. Loads one artifact the way a consumer does, by
// importing its glue and awaiting the default init, and exposes a small API
// on `window.scolta` for the tests and the benchmark to drive.

export interface CallResult {
  output?: string;
  error?: string;
  missing?: true;
}

export interface DeadlineResult {
  status: "ok" | "error" | "timeout";
  ms: number;
  output?: string;
  error?: string;
}

interface Glue {
  default: (url?: string) => Promise<unknown>;
  [name: string]: unknown;
}

export interface LoadResult {
  ms?: number;
  error?: string;
}

export interface Harness {
  /** Load and initialize an artifact; resolves to the init time in ms. */
  load(route: string, stem: string): Promise<number>;
  /**
   * Load a pre-gzipped artifact through its loader, from `url` or, without
   * one, from the .gz beside the loader. Never rejects: a failure comes
   * back as its message.
   */
  loadVia(route: string, stem: string, url?: string): Promise<LoadResult>;
  call(fn: string, arg: string): CallResult;
  /**
   * Time `n` samples of `fn` on `arg`, each the mean over `batch` calls, in
   * ms. Batching keeps sub-millisecond calls measurable in engines that
   * coarsen `performance.now()` to a millisecond.
   */
  time(fn: string, arg: string, n: number, batch?: number): number[];
  exports(): string[];
  /** Run sanitize_query in a worker, terminated after `limitMs`. */
  sanitizeWithDeadline(route: string, stem: string, arg: string, limitMs: number): Promise<DeadlineResult>;
  violations: string[];
}

declare global {
  interface Window {
    scolta: Harness;
  }
}

let glue: Glue | undefined;
const violations: string[] = [];
document.addEventListener("securitypolicyviolation", (e) => {
  violations.push(`${e.violatedDirective} ${e.blockedURI}`);
});

function required(): Glue {
  if (glue === undefined) throw new Error("no artifact loaded");
  return glue;
}

window.scolta = {
  async load(route, stem) {
    const start = performance.now();
    glue = (await import(`/artifact/${route}/${stem}.js`)) as Glue;
    await glue.default();
    return performance.now() - start;
  },
  async loadVia(route, stem, url) {
    const start = performance.now();
    try {
      const loader = (await import(`/artifact/${route}/${stem}_load.js`)) as Glue;
      await (url === undefined ? loader.default() : loader.default(url));
      glue = loader;
      return { ms: performance.now() - start };
    } catch (e) {
      return { error: e instanceof Error ? e.message : String(e) };
    }
  },
  call(fn, arg) {
    const f = required()[fn];
    if (typeof f !== "function") return { missing: true };
    try {
      return { output: String((f as (a: string) => unknown)(arg)) };
    } catch (e) {
      return { error: e instanceof Error ? e.message : String(e) };
    }
  },
  time(fn, arg, n, batch = 1) {
    const f = required()[fn] as (a: string) => unknown;
    const samples: number[] = [];
    for (let i = 0; i < n; i++) {
      const start = performance.now();
      for (let j = 0; j < batch; j++) f(arg);
      samples.push((performance.now() - start) / batch);
    }
    return samples;
  },
  exports() {
    return Object.keys(required()).filter((k) => typeof required()[k] === "function");
  },
  sanitizeWithDeadline(route, stem, arg, limitMs) {
    return new Promise((resolve) => {
      const worker = new Worker(`/harness/sanitize-worker.js?route=${route}&stem=${stem}`, { type: "module" });
      let timer: ReturnType<typeof setTimeout> | undefined;
      let start = 0;
      worker.onmessage = (e: MessageEvent<{ ready?: true; output?: string; error?: string }>) => {
        if (e.data.ready) {
          // The clock starts once the module is up: the deadline is for the
          // call, and it runs here, on a thread the call cannot block.
          start = performance.now();
          timer = setTimeout(() => {
            worker.terminate();
            resolve({ status: "timeout", ms: performance.now() - start });
          }, limitMs);
          worker.postMessage(arg);
          return;
        }
        clearTimeout(timer);
        worker.terminate();
        const ms = performance.now() - start;
        resolve(
          e.data.error === undefined
            ? { status: "ok", ms, output: e.data.output ?? "" }
            : { status: "error", ms, error: e.data.error },
        );
      };
    });
  },
  violations,
};
