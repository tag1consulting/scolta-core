// Module worker: loads an artifact, then runs sanitize_query on each message.
// Used for inputs that might hang, so the page can enforce a deadline from a
// thread the call does not block.

const params = new URL(import.meta.url).searchParams;
const route = params.get("route") ?? "";
const stem = params.get("stem") ?? "";

const glue = (await import(`/artifact/${route}/${stem}.js`)) as {
  default: () => Promise<unknown>;
  sanitize_query: (arg: string) => string;
};
await glue.default();

// The project compiles against the DOM lib, not the worker lib; this is the
// slice of a worker scope used here.
const scope = self as unknown as {
  postMessage(message: unknown): void;
  onmessage: ((e: MessageEvent<string>) => void) | null;
};

scope.onmessage = (e) => {
  try {
    scope.postMessage({ output: glue.sanitize_query(e.data) });
  } catch (err) {
    scope.postMessage({ error: err instanceof Error ? err.message : String(err) });
  }
};
scope.postMessage({ ready: true });
