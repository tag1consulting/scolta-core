//! Language-aware stop word lists.
//!
//! Provides static stop word arrays for 30 languages (ISO 639-1 codes).
//! All entries are lowercase. The `get_stop_words` function is the single
//! entry point for the rest of the crate.
//!
//! CJK languages (zh, ja, ko) and unknown codes return an empty slice —
//! word-boundary stop word filtering is not meaningful for those scripts.

/// Return the stop word list for the given ISO 639-1 language code.
///
/// # Arguments
/// * `language` - ISO 639-1 code (e.g. `"en"`, `"de"`, `"fr"`)
///
/// # Returns
/// A static slice of lowercase stop words, or `&[]` for unknown languages
/// and CJK (zh, ja, ko).
pub fn get_stop_words(language: &str) -> &'static [&'static str] {
    match language {
        "ar" => STOP_WORDS_AR,
        "ca" => STOP_WORDS_CA,
        "da" => STOP_WORDS_DA,
        "de" => STOP_WORDS_DE,
        "el" => STOP_WORDS_EL,
        "en" => STOP_WORDS_EN,
        "es" => STOP_WORDS_ES,
        "et" => STOP_WORDS_ET,
        "eu" => STOP_WORDS_EU,
        "fi" => STOP_WORDS_FI,
        "fr" => STOP_WORDS_FR,
        "ga" => STOP_WORDS_GA,
        "hi" => STOP_WORDS_HI,
        "hu" => STOP_WORDS_HU,
        "hy" => STOP_WORDS_HY,
        "id" => STOP_WORDS_ID,
        "it" => STOP_WORDS_IT,
        "lt" => STOP_WORDS_LT,
        "ne" => STOP_WORDS_NE,
        "nl" => STOP_WORDS_NL,
        "no" => STOP_WORDS_NO,
        "pl" => STOP_WORDS_PL,
        "pt" => STOP_WORDS_PT,
        "ro" => STOP_WORDS_RO,
        "ru" => STOP_WORDS_RU,
        "sr" => STOP_WORDS_SR,
        "sv" => STOP_WORDS_SV,
        "ta" => STOP_WORDS_TA,
        "tr" => STOP_WORDS_TR,
        "yi" => STOP_WORDS_YI,
        _ => &[], // Unknown languages and CJK (zh, ja, ko)
    }
}

// ---------------------------------------------------------------------------
// English
// ---------------------------------------------------------------------------

const STOP_WORDS_EN: &[&str] = &[
    // Articles
    "a", "an", "the", // Conjunctions
    "and", "but", "or", "nor", // Common prepositions
    "about", "as", "at", "by", "during", "for", "from", "if", "in", "into", "of", "on", "out",
    "through", "to", "up", "with", // Pronouns
    "he", "i", "it", "she", "that", "these", "they", "this", "those", "we", "what", "which", "who",
    "you", // Auxiliary/modal verbs
    "are", "be", "can", "could", "did", "do", "does", "has", "have", "is", "may", "might",
    "should", "was", "were", "will", "would", // Question words
    "how", "when", "where", "why", // Common filler
    "also", "just", "more", "most", "no", "not", "only", "same", "than", "very",
];

// ---------------------------------------------------------------------------
// German
// ---------------------------------------------------------------------------

const STOP_WORDS_DE: &[&str] = &[
    // Articles
    "der", "die", "das", "dem", "den", "des", "ein", "eine", "einem", "einen", "einer", "eines",
    // Conjunctions
    "und", "oder", "aber", "doch", "denn", "weil", "dass", "ob", "wenn", "als",
    // Prepositions
    "an", "auf", "aus", "bei", "bis", "durch", "für", "gegen", "in", "mit", "nach", "seit", "unter",
    "von", "vor", "zu", "zwischen", "über", // Pronouns
    "ich", "du", "er", "sie", "es", "wir", "ihr", "sich", "was", "wer", "welche", "welcher",
    "welches", "dieser", "diese", "dieses", "jener", "jene", "jenes", // Auxiliaries
    "bin", "bist", "ist", "sind", "seid", "war", "waren", "wäre", "haben", "hat", "hatte",
    "werden", "wird", "wurde", "sein", "kann", "könnte", "soll", "sollte", "muss", "müsste",
    "darf", "mag", "will", // Question words
    "wie", "wo", "wann", "warum", "woher", "wohin", // Filler
    "nicht", "auch", "noch", "schon", "nur", "sehr", "mehr", "bereits", "immer", "hier", "da",
    "dann", "nun", "so", "ja", "nein",
];

// ---------------------------------------------------------------------------
// French
// ---------------------------------------------------------------------------

const STOP_WORDS_FR: &[&str] = &[
    // Articles
    "le", "la", "les", "l", "un", "une", "des", "du", "de", // Conjunctions
    "et", "ou", "mais", "ni", "car", "or", "donc", "que", "quand", "si", // Prepositions
    "à", "au", "aux", "de", "du", "des", "en", "par", "pour", "sur", "sous", "dans", "avec",
    "sans", "vers", "entre", "depuis", "pendant", "après", "avant", // Pronouns
    "je", "tu", "il", "elle", "nous", "vous", "ils", "elles", "on", "me", "te", "se", "lui",
    "leur", "y", "en", "qui", "que", "quoi", "dont", "où", "ce", "cet", "cette", "ces",
    // Auxiliaries
    "est", "sont", "était", "étaient", "être", "avoir", "a", "ont", "avait", "avaient", "sera",
    "seront", "peut", "peuvent", "doit", "doivent", "fait", "font",
    // Question words
    "comment", "pourquoi", "quand", "où", // Filler
    "ne", "pas", "plus", "très", "aussi", "déjà", "encore", "même", "tout", "tous", "toute",
    "toutes", "bien", "trop",
];

// ---------------------------------------------------------------------------
// Spanish
// ---------------------------------------------------------------------------

const STOP_WORDS_ES: &[&str] = &[
    // Articles
    "el", "la", "los", "las", "un", "una", "unos", "unas", // Conjunctions
    "y", "e", "o", "u", "pero", "ni", "que", "si", "aunque", "porque", "como",
    // Prepositions
    "a", "ante", "bajo", "con", "contra", "de", "del", "desde", "durante", "en", "entre", "hacia",
    "hasta", "para", "por", "sin", "sobre", "tras", // Pronouns
    "yo", "tú", "él", "ella", "nosotros", "vosotros", "ellos", "ellas", "me", "te", "se", "le",
    "lo", "les", "nos", "os", "quien", "quién", "cual", "cuál", "cuales", "cuáles", "que", "qué",
    "este", "esta", "estos", "estas", "ese", "esa", "esos", "esas", // Auxiliaries
    "es", "son", "era", "eran", "fue", "fueron", "ser", "estar", "hay", "ha", "han", "había",
    "puede", "pueden", "debe", "deben", "hace", "hacen", // Question words
    "cómo", "cuándo", "dónde", "por qué", // Filler
    "no", "también", "sí", "ya", "muy", "más", "menos", "solo", "tan", "todo", "todos",
];

// ---------------------------------------------------------------------------
// Italian
// ---------------------------------------------------------------------------

const STOP_WORDS_IT: &[&str] = &[
    // Articles
    "il", "lo", "la", "i", "gli", "le", "un", "uno", "una", // Conjunctions
    "e", "ed", "o", "ma", "né", "che", "se", "come", "quando", "perché", "quindi",
    // Prepositions
    "a", "ad", "di", "da", "in", "con", "su", "per", "tra", "fra", "al", "allo", "alla", "ai",
    "agli", "alle", "del", "dello", "della", "dei", "degli", "delle", "dal", "dallo", "dalla",
    "dai", "dagli", "dalle", "nel", "nello", "nella", "nei", "negli", "nelle", "sul", "sullo",
    "sulla", "sui", "sugli", "sulle", // Pronouns
    "io", "tu", "lui", "lei", "noi", "voi", "loro", "mi", "ti", "si", "ci", "vi", "li", "le",
    "chi", "che", "cui", "dove", "quello", "quella", "quelli", "quelle", "questo", "questa",
    "questi", "queste", // Auxiliaries
    "è", "sono", "era", "erano", "fu", "furono", "essere", "avere", "ha", "hanno", "aveva",
    "avevano", "può", "possono", "deve", "devono", // Filler
    "non", "anche", "già", "solo", "molto", "più", "meno", "tutto", "tutti", "bene", "così",
];

// ---------------------------------------------------------------------------
// Dutch
// ---------------------------------------------------------------------------

const STOP_WORDS_NL: &[&str] = &[
    // Articles
    "de", "het", "een", // Conjunctions
    "en", "of", "maar", "want", "dat", "als", "toen", "omdat", "hoewel", "dus",
    // Prepositions
    "aan", "bij", "door", "in", "met", "na", "naar", "om", "op", "over", "per", "te", "tot", "uit",
    "van", "voor", "zonder", // Pronouns
    "ik", "jij", "je", "hij", "zij", "ze", "wij", "we", "u", "hen", "hun", "zich", "die", "dat",
    "wie", "wat", "welk", "welke", "deze", "dit", "die", "dat", // Auxiliaries
    "is", "zijn", "was", "waren", "ben", "bent", "heeft", "hebben", "had", "hadden", "zal",
    "zullen", "zou", "zouden", "kan", "kunnen", "moet", "moeten", "mag", "mogen", "wil", "willen",
    // Filler
    "niet", "ook", "al", "nog", "meer", "zo", "heel", "heel", "er", "hier", "daar", "nu", "dan",
    "toch", "wel", "geen",
];

// ---------------------------------------------------------------------------
// Portuguese
// ---------------------------------------------------------------------------

const STOP_WORDS_PT: &[&str] = &[
    // Articles
    "o", "a", "os", "as", "um", "uma", "uns", "umas", // Conjunctions
    "e", "ou", "mas", "nem", "que", "se", "como", "quando", "porque", "porém", "portanto",
    // Prepositions
    "a", "de", "em", "por", "para", "com", "sem", "sobre", "sob", "entre", "até", "após", "ante",
    "perante", "ao", "à", "aos", "às", "do", "da", "dos", "das", "no", "na", "nos", "nas", "pelo",
    "pela", "pelos", "pelas", // Pronouns
    "eu", "tu", "ele", "ela", "nós", "vós", "eles", "elas", "me", "te", "se", "lhe", "nos", "vos",
    "lhes", "quem", "que", "qual", "quais", "este", "esta", "estes", "estas", "esse", "essa",
    "esses", "essas", "aquele", "aquela", // Auxiliaries
    "é", "são", "era", "eram", "foi", "foram", "ser", "estar", "tem", "têm", "tinha", "tinham",
    "pode", "podem", "deve", "devem", "há", // Filler
    "não", "também", "já", "muito", "mais", "menos", "tudo", "todos", "bem", "só", "ainda",
];

// ---------------------------------------------------------------------------
// Swedish
// ---------------------------------------------------------------------------

const STOP_WORDS_SV: &[&str] = &[
    // Articles
    "en", "ett", "den", "det", "de", // Conjunctions
    "och", "eller", "men", "att", "om", "när", "eftersom", "därför", "fast", "dock",
    // Prepositions
    "i", "på", "till", "från", "med", "av", "för", "vid", "under", "över", "efter", "innan", "utan",
    "mot", // Pronouns
    "jag", "du", "han", "hon", "vi", "ni", "de", "mig", "dig", "honom", "henne", "oss", "er",
    "dem", "sig", "vem", "vad", "vilken", "vilket", "vilka", "denna", "detta", "dessa",
    // Auxiliaries
    "är", "var", "var", "har", "hade", "ska", "skulle", "kan", "kunde", "måste", "bör", "vill",
    // Filler
    "inte", "också", "redan", "bara", "nog", "väl", "ju", "nog", "ännu", "mer", "mest", "här",
    "där", "nu", "då",
];

// ---------------------------------------------------------------------------
// Danish
// ---------------------------------------------------------------------------

const STOP_WORDS_DA: &[&str] = &[
    "en", "et", "den", "det", "de", "og", "eller", "men", "at", "om", "når", "fordi", "hvis",
    "men", "dog", "i", "på", "til", "fra", "med", "af", "for", "ved", "under", "over", "efter",
    "inden", "uden", "mod", "jeg", "du", "han", "hun", "vi", "de", "dem", "sig", "hvem", "hvad",
    "hvilken", "hvilket", "disse", "dette", "er", "var", "har", "havde", "skal", "skulle", "kan",
    "kunne", "vil", "ville", "ikke", "også", "allerede", "kun", "endnu", "mere", "her", "der",
    "nu", "da",
];

// ---------------------------------------------------------------------------
// Norwegian
// ---------------------------------------------------------------------------

const STOP_WORDS_NO: &[&str] = &[
    "en", "ei", "et", "den", "det", "de", "og", "eller", "men", "at", "om", "når", "fordi", "hvis",
    "dog", "i", "på", "til", "fra", "med", "av", "for", "ved", "under", "over", "etter", "uten",
    "mot", "jeg", "du", "han", "hun", "vi", "dere", "de", "dem", "seg", "hvem", "hva", "hvilken",
    "hvilket", "dette", "disse", "er", "var", "har", "hadde", "skal", "skulle", "kan", "kunne",
    "vil", "ville", "ikke", "også", "allerede", "bare", "ennå", "mer", "her", "der", "nå", "da",
];

// ---------------------------------------------------------------------------
// Finnish
// ---------------------------------------------------------------------------

const STOP_WORDS_FI: &[&str] = &[
    "ja",
    "tai",
    "mutta",
    "että",
    "jos",
    "kun",
    "koska",
    "kuin",
    "vaikka",
    "se",
    "ne",
    "hän",
    "he",
    "me",
    "te",
    "minä",
    "sinä",
    "mikä",
    "kuka",
    "joka",
    "jotka",
    "tämä",
    "nämä",
    "tuo",
    "nuo",
    "on",
    "olla",
    "oli",
    "ovat",
    "olivat",
    "ollut",
    "ole",
    "ei",
    "en",
    "et",
    "emme",
    "ette",
    "eivät",
    "voi",
    "voida",
    "pitää",
    "täytyy",
    "saada",
    "missä",
    "milloin",
    "miten",
    "miksi",
    "minne",
    "myös",
    "vain",
    "jo",
    "vielä",
    "enemmän",
    "hyvin",
    "niin",
    "kaikki",
    "aina",
    "sitten",
    "nyt",
    "tässä",
    "siellä",
    "täällä",
];

// ---------------------------------------------------------------------------
// Catalan
// ---------------------------------------------------------------------------

const STOP_WORDS_CA: &[&str] = &[
    "el",
    "la",
    "els",
    "les",
    "un",
    "una",
    "uns",
    "unes",
    "l",
    "i",
    "o",
    "però",
    "ni",
    "que",
    "si",
    "com",
    "quan",
    "perquè",
    "doncs",
    "a",
    "de",
    "del",
    "d",
    "en",
    "al",
    "per",
    "amb",
    "sense",
    "sobre",
    "sota",
    "entre",
    "fins",
    "des de",
    "jo",
    "tu",
    "ell",
    "ella",
    "nosaltres",
    "vosaltres",
    "ells",
    "elles",
    "em",
    "et",
    "es",
    "li",
    "ens",
    "us",
    "els",
    "qui",
    "qual",
    "quals",
    "aquest",
    "aquesta",
    "aquests",
    "aquestes",
    "aquell",
    "aquella",
    "és",
    "son",
    "era",
    "eren",
    "va ser",
    "ser",
    "estar",
    "ha",
    "han",
    "havia",
    "havien",
    "pot",
    "poden",
    "ha de",
    "cal",
    "no",
    "també",
    "ja",
    "molt",
    "més",
    "menys",
    "tot",
    "tots",
    "tota",
    "totes",
    "bé",
    "massa",
];

// ---------------------------------------------------------------------------
// Romanian
// ---------------------------------------------------------------------------

const STOP_WORDS_RO: &[&str] = &[
    "un", "o", "unei", "unui", "unor", "unii", "unele", "și", "sau", "dar", "nici", "că", "dacă",
    "cum", "când", "deoarece", "deci", "la", "în", "pe", "de", "din", "spre", "cu", "fără",
    "între", "după", "înainte", "eu", "tu", "el", "ea", "noi", "voi", "ei", "ele", "mă", "te",
    "se", "îl", "o", "ne", "vă", "îi", "le", "care", "ce", "cine", "această", "acest", "aceste",
    "acești", "este", "sunt", "era", "erau", "a fost", "fi", "are", "au", "avea", "aveau", "poate",
    "pot", "trebuie", "vrea", "nu", "și", "deja", "doar", "mai", "foarte", "tot", "toți", "toate",
    "bine", "prea",
];

// ---------------------------------------------------------------------------
// Polish
// ---------------------------------------------------------------------------

const STOP_WORDS_PL: &[&str] = &[
    "i",
    "lub",
    "ale",
    "ani",
    "bo",
    "że",
    "jeśli",
    "kiedy",
    "jak",
    "ponieważ",
    "więc",
    "w",
    "na",
    "do",
    "z",
    "ze",
    "od",
    "dla",
    "przez",
    "po",
    "przed",
    "przy",
    "pod",
    "nad",
    "między",
    "za",
    "bez",
    "o",
    "o",
    "ja",
    "ty",
    "on",
    "ona",
    "ono",
    "my",
    "wy",
    "oni",
    "one",
    "się",
    "mnie",
    "cię",
    "go",
    "jej",
    "nam",
    "wam",
    "im",
    "kto",
    "co",
    "który",
    "która",
    "które",
    "ten",
    "ta",
    "to",
    "ci",
    "te",
    "jest",
    "są",
    "był",
    "była",
    "było",
    "byli",
    "były",
    "być",
    "ma",
    "mają",
    "miał",
    "miała",
    "może",
    "mogą",
    "musi",
    "trzeba",
    "można",
    "nie",
    "też",
    "już",
    "jeszcze",
    "bardzo",
    "więcej",
    "mniej",
    "cały",
    "wszystko",
    "zawsze",
    "tu",
    "tam",
    "teraz",
    "tak",
];

// ---------------------------------------------------------------------------
// Indonesian
// ---------------------------------------------------------------------------

const STOP_WORDS_ID: &[&str] = &[
    "dan", "atau", "tapi", "tetapi", "namun", "karena", "sebab", "jika", "kalau", "ketika", "maka",
    "sehingga", "di", "ke", "dari", "untuk", "dengan", "pada", "oleh", "dalam", "tentang",
    "terhadap", "antara", "bagi", "menurut", "melalui", "sejak", "saya", "aku", "kamu", "anda",
    "dia", "ia", "kami", "kita", "mereka", "yang", "ini", "itu", "tersebut", "para", "adalah",
    "ada", "tidak", "juga", "sudah", "akan", "bisa", "dapat", "harus", "perlu", "boleh", "ingin",
    "mau", "pun", "sangat", "lebih", "semua", "banyak", "sering", "masih", "belum", "sudah",
    "segera", "di sini", "di sana",
];

// ---------------------------------------------------------------------------
// Turkish
// ---------------------------------------------------------------------------

const STOP_WORDS_TR: &[&str] = &[
    "ve",
    "veya",
    "ama",
    "fakat",
    "lakin",
    "ancak",
    "çünkü",
    "eğer",
    "ki",
    "de",
    "da",
    "ile",
    "için",
    "gibi",
    "göre",
    "kadar",
    "karşı",
    "üzere",
    "arasında",
    "önce",
    "sonra",
    "sırasında",
    "ben",
    "sen",
    "o",
    "biz",
    "siz",
    "onlar",
    "bu",
    "şu",
    "bunlar",
    "şunlar",
    "ne",
    "kim",
    "hangi",
    "nasıl",
    "nerede",
    "ne zaman",
    "neden",
    "niçin",
    "var",
    "yok",
    "değil",
    "olmak",
    "oldu",
    "olacak",
    "olmak",
    "ise",
    "idi",
    "olan",
    "olarak",
    "daha",
    "en",
    "çok",
    "az",
    "bütün",
    "hepsi",
    "tüm",
    "bazı",
    "hiç",
    "bir",
    "iki",
    "her",
    "başka",
    "diğer",
    "böyle",
    "şöyle",
    "artık",
    "bile",
    "zaten",
    "sadece",
    "sadece",
];

// ---------------------------------------------------------------------------
// Hungarian
// ---------------------------------------------------------------------------

const STOP_WORDS_HU: &[&str] = &[
    "és", "vagy", "de", "hanem", "mert", "ha", "amikor", "hogy", "mint", "tehát", "azonban", "ban",
    "ben", "ba", "be", "ra", "re", "hoz", "hez", "höz", "ból", "ből", "ról", "ről", "tól", "től",
    "nál", "nél", "nak", "nek", "val", "vel", "ért", "ig", "én", "te", "ő", "mi", "ti", "ők",
    "aki", "ami", "amely", "ez", "az", "ezek", "azok", "ki", "mi", "melyik", "hol", "mikor",
    "hogyan", "miért", "van", "nincs", "volt", "lesz", "lett", "lenni", "lehet", "kell", "szabad",
    "akar", "nem", "is", "már", "még", "csak", "nagyon", "igen", "mindent", "minden", "itt", "ott",
];

// ---------------------------------------------------------------------------
// Lithuanian
// ---------------------------------------------------------------------------

const STOP_WORDS_LT: &[&str] = &[
    "ir", "arba", "bet", "tačiau", "nes", "jei", "kai", "kaip", "kad", "todėl", "nors", "į", "iš",
    "per", "dėl", "be", "ties", "apie", "prie", "prieš", "po", "tarp", "iki", "su", "už", "link",
    "aš", "tu", "jis", "ji", "mes", "jūs", "jie", "jos", "kas", "kuris", "kuri", "kurie", "kurios",
    "šis", "ši", "šie", "šios", "tas", "ta", "tie", "tos", "yra", "buvo", "bus", "būti", "nebūti",
    "turi", "turėti", "gali", "galėti", "reikia", "ne", "taip", "dar", "jau", "tik", "labai",
    "daugiau", "visi", "viskas", "čia", "ten", "dabar", "tada",
];

// ---------------------------------------------------------------------------
// Estonian
// ---------------------------------------------------------------------------

const STOP_WORDS_ET: &[&str] = &[
    "ja", "või", "aga", "kuid", "sest", "et", "kui", "kuna", "kuigi", "siis", "nii", "seega", "in",
    "on", "at", "de", "ma", "sa", "ta", "me", "te", "nad", "kes", "mis", "milline", "see", "need",
    "selle", "nende", "mina", "sina", "on", "oli", "olid", "olen", "oled", "oleme", "olete",
    "olema", "ei", "pole", "saab", "saama", "peab", "pidama", "võib", "võima", "ka", "veel",
    "juba", "ainult", "kogu", "kõik", "väga", "rohkem", "siin", "seal", "nüüd",
];

// ---------------------------------------------------------------------------
// Basque
// ---------------------------------------------------------------------------

const STOP_WORDS_EU: &[&str] = &[
    "eta",
    "edo",
    "baina",
    "ala",
    "baldin",
    "gero",
    "orduan",
    "beraz",
    "nahiz",
    "da",
    "dira",
    "zen",
    "ziren",
    "izango",
    "izaten",
    "egon",
    "dago",
    "daude",
    "ez",
    "bai",
    "oso",
    "bat",
    "bi",
    "batzuk",
    "guztiak",
    "hau",
    "hori",
    "hura",
    "hauek",
    "horiek",
    "haiek",
    "nik",
    "zuk",
    "hark",
    "guk",
    "zuek",
    "haiek",
    "nork",
    "non",
    "noiz",
    "nola",
    "zergatik",
    "ere",
    "baino",
    "bezala",
    "bakarrik",
    "oraindik",
    "dagoeneko",
    "beti",
    "inoiz",
    "hemen",
    "hor",
    "han",
    "orain",
    "orduan",
];

// ---------------------------------------------------------------------------
// Irish (Gaelic)
// ---------------------------------------------------------------------------

const STOP_WORDS_GA: &[&str] = &[
    "agus",
    "nó",
    "ach",
    "mar",
    "is",
    "ní",
    "nach",
    "mura",
    "cé",
    "má",
    "nuair",
    "ag",
    "ar",
    "as",
    "chuig",
    "de",
    "do",
    "faoi",
    "fé",
    "go",
    "i",
    "idir",
    "le",
    "leis",
    "ó",
    "roimh",
    "thar",
    "trí",
    "um",
    "mé",
    "tú",
    "sé",
    "sí",
    "muid",
    "sinn",
    "sibh",
    "siad",
    "cé",
    "céard",
    "cad",
    "conas",
    "cathain",
    "cá",
    "an",
    "na",
    "na",
    "tá",
    "bhí",
    "beidh",
    "bheith",
    "nil",
    "ní",
    "nach",
    "ach",
    "mar",
    "féidir",
    "caithfidh",
    "déanann",
    "freisin",
    "fós",
    "cheana",
    "amháin",
    "go leor",
    "anseo",
    "ansin",
    "anois",
];

// ---------------------------------------------------------------------------
// Arabic
// ---------------------------------------------------------------------------

const STOP_WORDS_AR: &[&str] = &[
    // Articles / particles
    "ال",
    "و",
    "أو",
    "لا",
    "ما",
    "إن",
    "أن",
    "كان",
    // Prepositions
    "في",
    "من",
    "إلى",
    "على",
    "عن",
    "مع",
    "بين",
    "خلال",
    "بعد",
    "قبل",
    "حتى",
    "عند",
    "لدى",
    "ضد",
    "نحو",
    "حول",
    "منذ",
    "لأن",
    "بـ",
    "لـ",
    // Pronouns
    "هو",
    "هي",
    "هم",
    "هن",
    "هذا",
    "هذه",
    "هؤلاء",
    "ذلك",
    "تلك",
    "أولئك",
    "أنا",
    "أنت",
    "أنتِ",
    "نحن",
    "أنتم",
    "أنتن",
    "هما",
    // Auxiliaries
    "كانت",
    "كانوا",
    "يكون",
    "تكون",
    "كون",
    "هناك",
    "يوجد",
    "توجد",
    "ليس",
    // Question words
    "ما",
    "من",
    "كيف",
    "أين",
    "متى",
    "لماذا",
    "هل",
    // Filler
    "أيضاً",
    "أيضا",
    "فقط",
    "جداً",
    "جدا",
    "كل",
    "كلها",
    "ذلك",
    "ذاك",
    "بعض",
    "قد",
    "لقد",
    "لم",
    "لن",
    "إذ",
    "إذا",
    "أم",
    "ثم",
];

// ---------------------------------------------------------------------------
// Greek
// ---------------------------------------------------------------------------

const STOP_WORDS_EL: &[&str] = &[
    // Articles
    "ο",
    "η",
    "το",
    "οι",
    "τα",
    "τον",
    "την",
    "τους",
    "τις",
    "του",
    "της",
    "των",
    "ένας",
    "μία",
    "ένα",
    // Conjunctions
    "και",
    "ή",
    "αλλά",
    "ούτε",
    "μα",
    "ωστόσο",
    "ότι",
    "αν",
    "όταν",
    "γιατί",
    "εάν",
    "ενώ",
    // Prepositions
    "σε",
    "από",
    "για",
    "με",
    "χωρίς",
    "μέσα",
    "έξω",
    "μεταξύ",
    "μέχρι",
    "κατά",
    "περί",
    "κάτω",
    "πάνω",
    "πριν",
    "μετά",
    // Pronouns
    "εγώ",
    "εσύ",
    "αυτός",
    "αυτή",
    "αυτό",
    "εμείς",
    "εσείς",
    "αυτοί",
    "αυτές",
    "ποιος",
    "ποια",
    "ποιο",
    "τι",
    "που",
    "αυτό",
    "αυτή",
    // Auxiliaries
    "είναι",
    "ήταν",
    "θα",
    "έχει",
    "έχουν",
    "είχε",
    "είχαν",
    "μπορεί",
    "πρέπει",
    // Filler
    "δεν",
    "δε",
    "μην",
    "μη",
    "και",
    "επίσης",
    "πολύ",
    "πιο",
    "ήδη",
    "μόνο",
    "κάθε",
    "όλα",
    "εδώ",
    "εκεί",
    "τώρα",
];

// ---------------------------------------------------------------------------
// Hindi
// ---------------------------------------------------------------------------

const STOP_WORDS_HI: &[&str] = &[
    // Postpositions / conjunctions
    "का",
    "की",
    "के",
    "को",
    "से",
    "में",
    "पर",
    "तक",
    "के लिए",
    "और",
    "या",
    "लेकिन",
    "परंतु",
    "कि",
    "जो",
    "क्योंकि",
    "अगर",
    "जब",
    "जैसे",
    // Pronouns
    "मैं",
    "तुम",
    "आप",
    "वह",
    "वो",
    "हम",
    "वे",
    "यह",
    "ये",
    "उस",
    "उन",
    "जिस",
    "जिन",
    "कौन",
    "क्या",
    "कहाँ",
    "कब",
    "कैसे",
    "क्यों",
    // Auxiliaries
    "है",
    "हैं",
    "था",
    "थी",
    "थे",
    "हो",
    "हूँ",
    "होना",
    "नहीं",
    "मत",
    "कर",
    "किया",
    "होगा",
    "होगी",
    "होंगे",
    "सकता",
    "सकती",
    "सकते",
    "चाहिए",
    "पड़ता",
    // Filler
    "भी",
    "तो",
    "ही",
    "बहुत",
    "अधिक",
    "सभी",
    "कुछ",
    "यहाँ",
    "वहाँ",
    "अब",
];

// ---------------------------------------------------------------------------
// Armenian
// ---------------------------------------------------------------------------

const STOP_WORDS_HY: &[&str] = &[
    "և",
    "կամ",
    "բայց",
    "որ",
    "թե",
    "եթե",
    "երբ",
    "ինչ",
    "ով",
    "ուր",
    "ինչու",
    "ինչպես",
    "ի",
    "ից",
    "ով",
    "ում",
    "ի",
    "հետ",
    "մեջ",
    "վրա",
    "մոտ",
    "կողքին",
    "դիմաց",
    "ես",
    "դու",
    "նա",
    "մենք",
    "դուք",
    "նրանք",
    "սա",
    "դա",
    "նա",
    "սրանք",
    "դրանք",
    "է",
    "են",
    "էր",
    "էին",
    "լինի",
    "կա",
    "կան",
    "չկա",
    "չկան",
    "կարող",
    "պետք",
    "չէ",
    "ոչ",
    "նաև",
    "արդեն",
    "միայն",
    "շատ",
    "ավելի",
    "բոլոր",
    "ամբողջ",
    "այստեղ",
    "այնտեղ",
    "հիմա",
    "ավելի",
];

// ---------------------------------------------------------------------------
// Nepali
// ---------------------------------------------------------------------------

const STOP_WORDS_NE: &[&str] = &[
    "को",
    "का",
    "की",
    "लाई",
    "बाट",
    "मा",
    "सँग",
    "र",
    "वा",
    "तर",
    "किनकि",
    "यदि",
    "जब",
    "जस्तो",
    "जो",
    "जुन",
    "म",
    "तिमी",
    "तपाई",
    "उ",
    "हामी",
    "तिनीहरू",
    "यो",
    "त्यो",
    "यी",
    "ती",
    "के",
    "कुन",
    "कसरी",
    "कहाँ",
    "कहिले",
    "किन",
    "छ",
    "छन्",
    "थियो",
    "थिए",
    "हो",
    "होइन",
    "हुन्छ",
    "हुन्न",
    "सक्छ",
    "सकिन्छ",
    "पनि",
    "नि",
    "त",
    "धेरै",
    "सबै",
    "यहाँ",
    "त्यहाँ",
    "अहिले",
];

// ---------------------------------------------------------------------------
// Russian
// ---------------------------------------------------------------------------

const STOP_WORDS_RU: &[&str] = &[
    // Conjunctions
    "и",
    "или",
    "но",
    "что",
    "как",
    "когда",
    "если",
    "чтобы",
    "потому",
    "хотя",
    "раз",
    "либо",
    "ни",
    "да",
    "же",
    // Prepositions
    "в",
    "на",
    "с",
    "из",
    "от",
    "до",
    "за",
    "по",
    "под",
    "над",
    "перед",
    "при",
    "о",
    "об",
    "к",
    "у",
    "для",
    "без",
    "через",
    "между",
    "после",
    "про",
    // Pronouns
    "я",
    "ты",
    "он",
    "она",
    "мы",
    "вы",
    "они",
    "это",
    "то",
    "тот",
    "та",
    "те",
    "свой",
    "который",
    "которая",
    "которое",
    "которые",
    "что",
    "кто",
    "себя",
    // Auxiliaries
    "есть",
    "был",
    "была",
    "было",
    "были",
    "будет",
    "будут",
    "быть",
    "нет",
    "нету",
    "можно",
    "нельзя",
    "надо",
    "нужно",
    "является",
    "являются",
    // Filler
    "не",
    "уже",
    "ещё",
    "только",
    "очень",
    "более",
    "все",
    "всё",
    "всего",
    "здесь",
    "там",
    "так",
    "тоже",
    "также",
    "всегда",
    "часто",
    "много",
    "мало",
];

// ---------------------------------------------------------------------------
// Serbian (Cyrillic)
// ---------------------------------------------------------------------------

const STOP_WORDS_SR: &[&str] = &[
    "и",
    "или",
    "али",
    "јер",
    "ако",
    "када",
    "мада",
    "те",
    "па",
    "него",
    "у",
    "на",
    "са",
    "из",
    "до",
    "за",
    "по",
    "под",
    "над",
    "пред",
    "о",
    "од",
    "к",
    "ка",
    "без",
    "кроз",
    "међу",
    "после",
    "пре",
    "поред",
    "кад",
    "ја",
    "ти",
    "он",
    "она",
    "оно",
    "ми",
    "ви",
    "они",
    "оне",
    "она",
    "то",
    "овај",
    "ова",
    "ово",
    "тај",
    "та",
    "то",
    "овде",
    "тамо",
    "ко",
    "шта",
    "који",
    "која",
    "које",
    "је",
    "јесу",
    "би",
    "бити",
    "нема",
    "има",
    "имати",
    "могу",
    "може",
    "треба",
    "не",
    "да",
    "već",
    "само",
    "такође",
    "веома",
    "сви",
    "све",
    "свако",
    "ту",
    "ту",
];

// ---------------------------------------------------------------------------
// Tamil
// ---------------------------------------------------------------------------

const STOP_WORDS_TA: &[&str] = &[
    "மற்றும்",
    "அல்லது",
    "ஆனால்",
    "ஏனெனில்",
    "என்று",
    "எனவே",
    "அப்போது",
    "இருந்து",
    "இல்",
    "இன்",
    "ஆல்",
    "க்கு",
    "உடன்",
    "பற்றி",
    "மேல்",
    "கீழ்",
    "முன்",
    "பின்",
    "நான்",
    "நீ",
    "அவர்",
    "அவள்",
    "நாம்",
    "நீங்கள்",
    "அவர்கள்",
    "இது",
    "அது",
    "இந்த",
    "அந்த",
    "எது",
    "யார்",
    "எங்கே",
    "எப்போது",
    "எப்படி",
    "ஏன்",
    "இருக்கிறது",
    "இருந்தது",
    "இல்லை",
    "உள்ளது",
    "ஆகும்",
    "ஒரு",
    "மிகவும்",
    "எல்லா",
    "எல்லாம்",
    "இங்கே",
    "அங்கே",
    "இப்போது",
];

// ---------------------------------------------------------------------------
// Yiddish
// ---------------------------------------------------------------------------

const STOP_WORDS_YI: &[&str] = &[
    "און",
    "אָדער",
    "אָבער",
    "ווייל",
    "ווען",
    "אַז",
    "אויב",
    "ביז",
    "אין",
    "אויף",
    "פֿון",
    "צו",
    "מיט",
    "בײַ",
    "פֿאַר",
    "איבער",
    "אונטער",
    "נאָך",
    "צווישן",
    "קעגן",
    "אָן",
    "איך",
    "דו",
    "ער",
    "זי",
    "מיר",
    "איר",
    "זיי",
    "דאָס",
    "דאָ",
    "יענעם",
    "דעם",
    "וועלכע",
    "וואָס",
    "ווער",
    "ווי",
    "וואו",
    "ווען",
    "פֿאַרוואָס",
    "איז",
    "זײַנען",
    "ביסט",
    "זיי",
    "זיין",
    "ניט",
    "נישט",
    "האָט",
    "האָבן",
    "קען",
    "קענען",
    "מוז",
    "דאַרף",
    "אויך",
    "שוין",
    "נאָר",
    "זייער",
    "מער",
    "אַלע",
    "אַלץ",
    "דאָ",
    "דאָרט",
    "יעצט",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_stop_words_en() {
        let words = get_stop_words("en");
        assert!(!words.is_empty());
        assert!(words.contains(&"the"));
        assert!(words.contains(&"and"));
        assert!(words.contains(&"is"));
    }

    #[test]
    fn test_get_stop_words_de() {
        let words = get_stop_words("de");
        assert!(!words.is_empty());
        assert!(words.contains(&"der"));
        assert!(words.contains(&"und"));
        assert!(words.contains(&"nicht"));
    }

    #[test]
    fn test_get_stop_words_fr() {
        let words = get_stop_words("fr");
        assert!(!words.is_empty());
        assert!(words.contains(&"le"));
        assert!(words.contains(&"et"));
    }

    #[test]
    fn test_get_stop_words_unknown_returns_empty() {
        assert!(get_stop_words("xx").is_empty());
        assert!(get_stop_words("zh").is_empty());
        assert!(get_stop_words("ja").is_empty());
        assert!(get_stop_words("ko").is_empty());
        assert!(get_stop_words("").is_empty());
    }

    #[test]
    fn test_all_entries_lowercase() {
        for lang in &[
            "en", "de", "fr", "es", "it", "nl", "pt", "sv", "da", "no", "fi", "ca", "ro", "pl",
            "id", "tr", "hu", "lt", "et", "eu", "ga",
        ] {
            for word in get_stop_words(lang) {
                assert_eq!(
                    *word,
                    word.to_lowercase(),
                    "lang={}: '{}' is not lowercase",
                    lang,
                    word
                );
            }
        }
    }

    #[test]
    fn test_all_30_languages_nonempty() {
        for lang in &[
            "ar", "ca", "da", "de", "el", "en", "es", "et", "eu", "fi", "fr", "ga", "hi", "hu",
            "hy", "id", "it", "lt", "ne", "nl", "no", "pl", "pt", "ro", "ru", "sr", "sv", "ta",
            "tr", "yi",
        ] {
            assert!(
                !get_stop_words(lang).is_empty(),
                "lang={} returned empty stop words",
                lang
            );
        }
    }
}
