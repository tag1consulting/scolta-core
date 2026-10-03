// Fixed inputs for the search parity fixture.
//
// The inputs are generated, not captured: a seeded generator builds a small
// multilingual corpus and a set of calls over it, so the fixture is the same
// on every machine. Only the outputs are captured, from a named build (see
// capture-fixtures.ts). Every call here is one a browser consumer makes, plus
// the AI helper exports, which only the full artifact has.

export interface ParityCase {
  /** The export to call. */
  fn: string;
  /** Its single argument: passed as is when a string, else as JSON. */
  input: unknown;
}

/** Scoring reads the clock for recency, so every run pins it here. */
export const FIXED_NOW_MS = Date.UTC(2026, 9, 1, 12, 0, 0);

class Rng {
  private state: number;
  constructor(seed: number) {
    this.state = seed >>> 0 || 1;
  }
  next(): number {
    // xorshift32
    let x = this.state;
    x ^= x << 13;
    x ^= x >>> 17;
    x ^= x << 5;
    this.state = x >>> 0;
    return this.state;
  }
  below(n: number): number {
    return this.next() % n;
  }
  pick<T>(items: readonly T[]): T {
    const item = items[this.below(items.length)];
    if (item === undefined) throw new Error("pick from an empty list");
    return item;
  }
}

const TOPICS: readonly (readonly [string, readonly string[]])[] = [
  ["en", ["drupal", "performance", "caching", "migration", "security", "accessibility", "hosting", "database"]],
  ["en", ["chocolate", "cake", "vegan", "recipe", "baking", "lemon", "grill", "pizza"]],
  ["en", ["patient", "privacy", "hipaa", "records", "clinic", "insurance", "billing", "telehealth"]],
  ["de", ["über", "straße", "suche", "leistung", "datenbank", "sicherheit", "größe", "zugang"]],
  ["fr", ["recherche", "été", "sécurité", "données", "hôpital", "réseau", "cœur", "français"]],
  ["ar", ["ضريبة", "بحث", "قاعدة", "بيانات", "أمن", "شبكة", "مستشفى", "خصوصية"]],
  ["hi", ["खोज", "डेटा", "सुरक्षा", "नेटवर्क", "अस्पताल", "गोपनीयता", "बीमा", "प्रदर्शन"]],
  ["zh", ["数据库", "搜索", "安全", "网络", "医院", "隐私", "性能", "缓存"]],
];

const FILLER = [
  "the", "and", "with", "for", "a", "guide", "to", "how", "why", "our", "new", "2024", "v10", "best",
  "practices", "notes", "team", "update", "faq", "overview",
];

function sentence(rng: Rng, words: readonly string[], length: number): string {
  const out: string[] = [];
  for (let i = 0; i < length; i++) {
    out.push(rng.below(3) === 0 ? rng.pick(FILLER) : rng.pick(words));
  }
  const first = out[0] ?? "";
  return `${first.charAt(0).toUpperCase()}${first.slice(1)} ${out.slice(1).join(" ")}.`;
}

interface Doc {
  url: string;
  title: string;
  excerpt: string;
  date: string;
  content: string;
  language: string;
}

function corpus(rng: Rng): Doc[] {
  const docs: Doc[] = [];
  for (let i = 0; i < 72; i++) {
    const [language, words] = rng.pick(TOPICS);
    const year = 2018 + rng.below(9);
    const month = 1 + rng.below(12);
    const day = 1 + rng.below(28);
    const paragraphs = Array.from({ length: 3 + rng.below(4) }, () =>
      Array.from({ length: 2 + rng.below(3) }, () => sentence(rng, words, 6 + rng.below(10))).join(" "),
    );
    docs.push({
      url: `https://example.org/${language}/${i}${rng.below(4) === 0 ? "/" : ""}`,
      title: sentence(rng, words, 2 + rng.below(5)).replace(/\.$/, ""),
      excerpt: sentence(rng, words, 8 + rng.below(12)),
      date: rng.below(10) === 0 ? "" : `${year}-${String(month).padStart(2, "0")}-${String(day).padStart(2, "0")}`,
      content: paragraphs.join("\n\n"),
      language,
    });
  }
  return docs;
}

const QUERIES: readonly (readonly [string, string])[] = [
  ["drupal performance", "en"],
  ['"drupal performance"', "en"],
  ["caching migration security", "en"],
  ["vegan chocolate cake", "en"],
  ['"lemon cake"', "en"],
  ["hipaa patient records", "en"],
  ["telehealth billing insurance clinic", "en"],
  ["the", "en"],
  ["2024", "en"],
  ["v10 drupal", "en"],
  ["über straße", "de"],
  ["datenbank sicherheit", "de"],
  ["sécurité des données", "fr"],
  ["hôpital", "fr"],
  ["قاعدة بيانات", "ar"],
  ["डेटा सुरक्षा", "hi"],
  ["数据库 性能", "zh"],
  ["guide", "en"],
  ["nonexistentterm", "en"],
  ["", "en"],
];

/** Every parity case, in a stable order. */
export function parityCases(): ParityCase[] {
  const rng = new Rng(0x5c017a);
  const docs = corpus(rng);
  const asResult = (doc: Doc, withScore: boolean) => ({
    url: doc.url,
    title: doc.title,
    excerpt: doc.excerpt,
    date: doc.date,
    ...(withScore ? { score: (rng.below(1000) + 1) / 1000 } : {}),
  });
  const sample = (count: number): Doc[] => Array.from({ length: count }, () => rng.pick(docs));
  const priorityPages = [
    { url_pattern: "/en/1", keywords: ["drupal", "performance"], boost: 50 },
    { url_pattern: "/en/", keywords: ["vegan"], boost: 10, custom_excerpt: "Plant-based recipes." },
    { url_pattern: "/de/", keywords: ["straße", "suche"], boost: 20, page_id: "de-hub" },
  ];
  const configs: Record<string, unknown>[] = [
    {},
    { recency_strategy: "none" },
    { recency_strategy: "linear", recency_boost_max: 0.8 },
    { title_match_boost: 2.0, phrase_adjacent_multiplier: 4.0 },
    { custom_stop_words: ["guide", "notes"] },
    { priority_pages: priorityPages },
  ];

  const cases: ParityCase[] = [];
  const add = (fn: string, input: unknown) =>
    cases.push({ fn, input });

  QUERIES.forEach(([query, language], q) => {
    // Three of the configs per query, rotating, so each config meets every
    // kind of query without the fixture growing to the full product.
    for (const config of [0, 2, 4].map((k) => configs[(q + k) % configs.length])) {
      add("score_results", {
        query,
        results: sample(6).map((d) => asResult(d, false)),
        config: { language, ...config },
      });
    }
  });
  add("batch_score_results", {
    queries: QUERIES.slice(0, 10).map(([query, language]) => ({
      query,
      results: sample(6).map((d) => asResult(d, false)),
      config: { language },
    })),
    default_config: { recency_strategy: "exponential" },
  });
  for (let i = 0; i < 6; i++) {
    add("merge_results", {
      sets: [
        { results: sample(6).map((d) => asResult(d, true)), weight: 1.0 },
        { results: sample(6).map((d) => asResult(d, true)), weight: 0.7 },
        { results: sample(4).map((d) => asResult(d, true)), weight: 0.4 },
      ],
      ...(i % 2 === 0 ? { deduplicate_by: "url", normalize_urls: true } : { deduplicate_by: "title" }),
      ...(i % 3 === 0 ? { exclude_urls: [docs[0]?.url ?? ""] } : {}),
      case_sensitive: i === 5,
    });
  }
  for (const [query] of QUERIES) {
    add("match_priority_pages", { query, priority_pages: priorityPages });
  }
  for (const [query, language] of QUERIES.slice(0, 8)) {
    const doc = rng.pick(docs);
    add("extract_context", {
      content: doc.content,
      query,
      config: { language, max_length: 600 + rng.below(1200), intro_length: 200, snippet_radius: 120 },
    });
    add("batch_extract_context", {
      items: sample(3).map((d) => ({ content: d.content, url: d.url, title: d.title })),
      query,
      config: { max_length: 800 },
    });
  }
  for (const query of ["call 555-867-5309 or mail a.b@example.com", "ssn ١٢٣-٤٥-٦٧٨٩", "plain query"]) {
    add("sanitize_query", { query, config: { redact_phone: query.length % 2 === 0 } });
  }
  // AI helper exports: in the full artifact only.
  add("parse_expansion", '["drupal caching", "page cache", "drupal", "platform"]');
  add("parse_expansion", {
    text: '["und", "suche", "leistung"]',
    language: "de",
    generic_terms: ["leistung"],
    existing_terms: ["suche"],
  });
  add("parse_expansion", "drupal caching, page cache\nvarnish");
  add("truncate_conversation", {
    messages: Array.from({ length: 12 }, (_, i) => ({
      role: i % 2 === 0 ? "user" : "assistant",
      content: sentence(rng, ["drupal", "cache", "query"], 40 + i * 7),
    })),
    config: { max_length: 900, preserve_first_n: 2, removal_unit: 2 },
  });
  return cases;
}

/** The exports only the full artifact has. */
export const AI_EXPORTS: readonly string[] = ["parse_expansion", "resolve_prompt", "get_prompt", "truncate_conversation"];

/** The string a case passes to its export. */
export function argument(input: unknown): string {
  return typeof input === "string" ? input : JSON.stringify(input);
}
