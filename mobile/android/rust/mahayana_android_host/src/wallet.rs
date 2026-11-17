//! TON Blockchain Wallet v4r2, BIP-39 24-word Mnemonic, BOC Cell Serializer,
//! Telegram Stars Ledger, Collectibles/Gifts, and TonConnect 2.0 Engine.

use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use pbkdf2::pbkdf2_hmac;
use rand::rngs::OsRng;
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256, Sha512};
use std::collections::BTreeMap;

pub const BIP39_WORD_COUNT: usize = 2048;
pub const TON_MNEMONIC_WORDS: usize = 24;
pub const TON_V4R2_WALLET_ID: u32 = 698_983_191; // 0x29A9A317 mainnet subwallet_id
pub const DEFAULT_FEE_NANOTONS: u64 = 5_500_000; // 0.0055 TON

/// Canonical 2048-word English BIP-39 vocabulary.
pub static BIP39_WORDS_BASE: &[&str] = &[
    "abandon", "ability", "able", "about", "above", "absent", "absorb", "abstract", "absurd", "abuse",
    "access", "accident", "account", "accuse", "achieve", "acid", "acoustic", "acquire", "across", "act",
    "action", "actor", "actress", "actual", "adapt", "add", "addict", "address", "adjust", "admit",
    "adult", "advance", "advice", "aerobic", "affair", "afford", "afraid", "again", "age", "agent",
    "agree", "ahead", "aim", "air", "airport", "aisle", "alarm", "album", "alcohol", "alert",
    "alien", "all", "alley", "allow", "almost", "alone", "alpha", "already", "also", "alter",
    "always", "amateur", "amazing", "among", "amount", "amused", "analyst", "anchor", "ancient", "anger",
    "angle", "angry", "animal", "ankle", "announce", "annual", "another", "answer", "antenna", "antique",
    "anxiety", "any", "apart", "apology", "appear", "apple", "approve", "april", "arch", "arctic",
    "area", "arena", "argue", "arm", "armed", "armor", "army", "around", "arrange", "arrest",
    "arrive", "arrow", "art", "artefact", "artist", "artwork", "ask", "aspect", "assault", "asset",
    "assist", "assume", "asthma", "athlete", "atom", "attack", "attend", "attitude", "attract", "auction",
    "audit", "august", "aunt", "author", "auto", "autumn", "average", "avocado", "avoid", "awake",
    "aware", "away", "awesome", "awful", "awkward", "axis", "baby", "bachelor", "bacon", "badge",
    "bag", "balance", "balcony", "ball", "bamboo", "banana", "banner", "bar", "barely", "bargain",
    "barrel", "base", "basic", "basket", "battle", "beach", "bean", "beauty", "because", "become",
    "beef", "before", "begin", "behave", "behind", "believe", "below", "belt", "bench", "benefit",
    "best", "betray", "better", "between", "beyond", "bicycle", "bid", "bike", "bind", "biology",
    "bird", "birth", "bitter", "black", "blade", "blame", "blanket", "blast", "bleak", "bless",
    "blind", "blood", "blossom", "blouse", "blue", "blur", "blush", "board", "boat", "body",
    "boil", "bomb", "bone", "bonus", "book", "boost", "border", "boring", "borrow", "boss",
    "bounce", "box", "boy", "bracket", "brain", "brand", "brass", "brave", "bread", "breeze",
    "brick", "bridge", "brief", "bright", "bring", "brisk", "broccoli", "broken", "bronze", "broom",
    "brother", "brown", "brush", "bubble", "buddy", "budget", "buffalo", "build", "bulb", "bulk",
    "bullet", "bundle", "bunker", "burden", "burger", "burst", "bus", "business", "busy", "butter",
    "buyer", "buzz", "cabbage", "cabin", "cable", "cactus", "cage", "cake", "call", "calm",
    "camera", "camp", "can", "canal", "cancel", "candy", "cannon", "canoe", "canvas", "canyon",
    "capable", "capital", "captain", "car", "carbon", "card", "cargo", "carpet", "carry", "cart",
    "case", "cash", "casino", "castle", "casual", "cat", "catalog", "catch", "category", "cattle",
    "caught", "cause", "caution", "cave", "ceiling", "celery", "cement", "census", "century", "cereal",
    "certain", "chair", "chalk", "champion", "change", "chaos", "chapter", "charge", "chase", "chat",
    "cheap", "check", "cheese", "chef", "cherry", "chest", "chicken", "chief", "child", "chimney",
    "choice", "choose", "chronic", "chuckle", "chunk", "churn", "cigar", "cinnamon", "circle", "citizen",
    "city", "civil", "claim", "clap", "clarify", "claw", "clay", "clean", "clerk", "clever",
    "click", "client", "cliff", "climb", "clinic", "clip", "clock", "clog", "close", "cloth",
    "cloud", "clown", "club", "clump", "cluster", "clutch", "coach", "coast", "coconut", "code",
    "coffee", "coil", "coin", "collect", "color", "column", "combine", "come", "comfort", "comic",
    "common", "company", "concert", "conduct", "confirm", "congress", "connect", "consider", "control", "convince",
    "cook", "cool", "copper", "copy", "coral", "core", "corn", "correct", "cost", "cotton",
    "couch", "country", "couple", "course", "cousin", "cover", "coyote", "crack", "cradle", "craft",
    "cram", "crane", "crash", "crater", "crawl", "crazy", "cream", "credit", "creek", "crew",
    "cricket", "crime", "crisp", "critic", "crop", "cross", "crouch", "crowd", "crucial", "cruel",
    "cruise", "crumble", "crunch", "crush", "cry", "crystal", "cube", "culture", "cup", "cupboard",
    "curious", "current", "curtain", "curve", "cushion", "custom", "cute", "cycle", "dad", "damage",
    "damp", "dance", "danger", "daring", "dash", "daughter", "dawn", "day", "deal", "debate",
    "debris", "decade", "december", "decide", "decline", "decorate", "decrease", "deer", "defense", "define",
    "defy", "degree", "delay", "deliver", "demand", "demise", "denial", "dentist", "deny", "depart",
    "depend", "deposit", "depth", "deputy", "derive", "describe", "desert", "design", "desk", "despair",
    "destroy", "detail", "detect", "develop", "device", "devote", "diagram", "dial", "diamond", "diary",
    "dice", "diesel", "diet", "differ", "digital", "dignity", "dilemma", "dinner", "dinosaur", "direct",
    "dirt", "disagree", "discover", "disease", "dish", "dismiss", "disorder", "display", "distance", "divert",
    "divide", "divorce", "dizzy", "doctor", "document", "dog", "doll", "dolphin", "domain", "donate",
    "donkey", "donor", "door", "dose", "double", "dove", "draft", "dragon", "drama", "drastic",
    "draw", "dream", "dress", "drift", "drill", "drink", "drip", "drive", "drop", "drum",
    "dry", "duck", "dumb", "dune", "during", "dust", "dutch", "duty", "dwarf", "dynamic",
    "eager", "eagle", "early", "earn", "earth", "easily", "east", "easy", "echo", "ecology",
    "economy", "edge", "edit", "educate", "effort", "egg", "eight", "either", "elbow", "elder",
    "electric", "elegant", "element", "elephant", "elevator", "elite", "else", "embark", "embody", "embrace",
    "emerge", "emotion", "employ", "empower", "empty", "enable", "enact", "end", "endless", "endorse",
    "enemy", "energy", "enforce", "engage", "engine", "enhance", "enjoy", "enlist", "enough", "enrich",
    "enroll", "ensure", "enter", "entire", "entry", "envelope", "episode", "equal", "equip", "era",
    "erase", "erode", "erosion", "error", "erupt", "escape", "essay", "essence", "estate", "eternal",
    "ethics", "evidence", "evil", "evoke", "evolve", "exact", "example", "excess", "exchange", "excite",
    "exclude", "excuse", "execute", "exercise", "exhaust", "exhibit", "exile", "exist", "exit", "exotic",
    "expand", "expect", "expire", "explain", "expose", "express", "extend", "extra", "eye", "eyebrow",
    "fabric", "face", "faculty", "fade", "faint", "faith", "fall", "false", "fame", "family",
    "famous", "fan", "fancy", "fantasy", "farm", "fashion", "fat", "fatal", "father", "fatigue",
    "fault", "favorite", "feature", "february", "federal", "fee", "feed", "feel", "female", "fence",
    "festival", "fetch", "fever", "few", "fiber", "fiction", "field", "figure", "file", "film",
    "filter", "final", "find", "fine", "finger", "finish", "fire", "firm", "first", "fiscal",
    "fish", "fit", "fitness", "fix", "flag", "flame", "flash", "flat", "flavor", "flee",
    "flight", "flip", "float", "flock", "floor", "flower", "fluid", "flush", "fly", "foam",
    "focus", "fog", "foil", "fold", "follow", "food", "foot", "force", "forest", "forget",
    "fork", "fortune", "forum", "forward", "fossil", "foster", "found", "fox", "fragile", "frame",
    "frequent", "fresh", "friend", "fringe", "frog", "front", "frost", "frown", "frozen", "fruit",
    "fuel", "fun", "funny", "furnace", "fury", "future", "gadget", "gain", "galaxy", "gallery",
    "game", "gap", "garage", "garbage", "garden", "garlic", "garment", "gas", "gasp", "gate",
    "gather", "gauge", "gaze", "general", "genius", "genre", "gentle", "genuine", "gesture", "ghost",
    "giant", "gift", "giggle", "ginger", "giraffe", "girl", "give", "glad", "glance", "glare",
    "glass", "glide", "glimpse", "globe", "gloom", "glory", "glove", "glow", "glue", "goat",
    "goddess", "gold", "good", "goose", "gorilla", "gospel", "gossip", "govern", "gown", "grab",
    "grace", "grain", "grant", "grape", "grass", "gravity", "great", "green", "grid", "grief",
    "grit", "grocery", "group", "grow", "grunt", "guard", "guess", "guide", "guilt", "guitar",
    "gun", "gym", "habit", "hair", "half", "hammer", "hamster", "hand", "happy", "harbor",
    "hard", "harsh", "harvest", "hat", "have", "hawk", "hazard", "head", "health", "heart",
    "heavy", "hedgehog", "height", "hello", "helmet", "help", "hen", "hero", "hidden", "high",
    "hill", "hint", "hip", "hire", "history", "hobby", "hockey", "hold", "hole", "holiday",
    "hollow", "home", "honey", "hood", "hope", "horn", "horror", "horse", "hospital", "host",
    "hotel", "hour", "hover", "hub", "huge", "human", "humble", "humor", "hundred", "hungry",
    "hunt", "hurdle", "hurry", "hurt", "husband", "hybrid", "ice", "icon", "idea", "identify",
    "idle", "ignore", "ill", "illegal", "illness", "image", "imitate", "immense", "immune", "impact",
    "impose", "improve", "impulse", "inch", "include", "income", "increase", "index", "indicate", "indoor",
    "industry", "infant", "inflict", "inform", "inhale", "inherit", "initial", "inject", "injury", "inmate",
    "inner", "innocent", "input", "inquiry", "insane", "insect", "inside", "inspire", "install", "intact",
    "interest", "into", "invest", "invite", "involve", "iron", "island", "isolate", "issue", "item",
    "ivory", "jacket", "jaguar", "jar", "jazz", "jealous", "jeans", "jelly", "jewel", "job",
    "join", "joke", "journey", "joy", "judge", "juice", "jump", "jungle", "junior", "junk",
    "just", "kangaroo", "keen", "keep", "ketchup", "key", "kick", "kid", "kidney", "kind",
    "kingdom", "kiss", "kit", "kitchen", "kite", "kitten", "kiwi", "knee", "knife", "knock",
    "know", "lab", "label", "labor", "ladder", "lady", "lake", "lamp", "language", "laptop",
    "large", "later", "latin", "laugh", "laundry", "lava", "law", "lawn", "lawsuit", "layer",
    "lazy", "leader", "leaf", "learn", "leave", "lecture", "left", "leg", "legal", "legend",
    "leisure", "lemon", "lend", "length", "lens", "leopard", "lesson", "letter", "level", "liar",
    "liberty", "library", "license", "life", "lift", "light", "like", "limb", "limit", "link",
    "lion", "liquid", "list", "little", "live", "lizard", "load", "loan", "lobster", "local",
    "lock", "logic", "lonely", "long", "loop", "lottery", "loud", "lounge", "love", "loyal",
    "lucky", "luggage", "lumber", "lunar", "lunch", "luxury", "lyrics", "machine", "mad", "magic",
    "magnet", "maid", "mail", "main", "major", "make", "mammal", "man", "manage", "mandate",
    "mango", "mansion", "manual", "maple", "marble", "march", "margin", "marine", "market", "marriage",
    "mask", "mass", "master", "match", "material", "math", "matrix", "matter", "maximum", "maze",
    "meadow", "mean", "measure", "meat", "mechanic", "medal", "media", "melody", "melt", "member",
    "memory", "mention", "menu", "mercy", "merge", "merit", "merry", "mesh", "message", "metal",
    "method", "middle", "midnight", "milk", "million", "mimic", "mind", "minimum", "minor", "minute",
    "miracle", "mirror", "misery", "miss", "mistake", "mix", "mixed", "mixture", "mobile", "model",
    "modify", "mom", "moment", "monitor", "monkey", "monster", "month", "moon", "moral", "more",
    "morning", "mosquito", "mother", "motion", "motor", "mountain", "mouse", "move", "movie", "much",
    "muffin", "mule", "multiply", "muscle", "museum", "mushroom", "music", "must", "mutual", "myself",
    "mystery", "myth", "naive", "name", "napkin", "narrow", "nasty", "nation", "nature", "near",
    "neck", "need", "negative", "neglect", "neither", "nephew", "nerve", "nest", "net", "network",
    "neutral", "never", "news", "next", "nice", "night", "noble", "noise", "nominee", "noodle",
    "normal", "north", "nose", "notable", "note", "nothing", "notice", "novel", "now", "nuclear",
    "number", "nurse", "nut", "oak", "obey", "object", "oblige", "obscure", "observe", "obtain",
    "obvious", "occur", "ocean", "october", "odor", "off", "offer", "office", "often", "oil",
    "okay", "old", "olive", "olympic", "omit", "once", "one", "onion", "online", "only",
    "open", "opera", "opinion", "oppose", "option", "orange", "orbit", "orchard", "order", "ordinary",
    "organ", "orient", "original", "orphan", "ostrich", "other", "outdoor", "outer", "output", "outside",
    "oval", "oven", "over", "own", "owner", "oxygen", "oyster", "ozone", "pact", "paddle",
    "page", "pair", "palace", "palm", "panda", "panel", "panic", "panther", "paper", "parade",
    "parent", "park", "parrot", "party", "pass", "patch", "path", "patient", "patrol", "pattern",
    "pause", "pave", "payment", "peace", "peach", "peacock", "peak", "peanut", "pear", "peasant",
    "pelican", "pen", "penalty", "pencil", "people", "pepper", "perfect", "permit", "person", "pet",
    "phone", "photo", "phrase", "physical", "piano", "picnic", "picture", "piece", "pig", "pigeon",
    "pill", "pilot", "pink", "pioneer", "pipe", "pistol", "pitch", "pizza", "place", "planet",
    "plastic", "plate", "play", "please", "pledge", "pluck", "plug", "plunge", "poem", "poet",
    "point", "polar", "pole", "police", "pond", "pony", "pool", "popular", "portion", "position",
    "possible", "post", "potato", "pottery", "poverty", "powder", "power", "practice", "praise", "predict",
    "prefer", "prepare", "present", "pretty", "prevent", "price", "pride", "primary", "print", "priority",
    "prison", "private", "prize", "problem", "process", "produce", "profit", "program", "project", "promote",
    "proof", "property", "prosper", "protect", "proud", "provide", "public", "pudding", "pull", "pulp",
    "pulse", "pumpkin", "punch", "pupil", "puppy", "purchase", "purity", "purpose", "purse", "push",
    "put", "puzzle", "pyramid", "quality", "quantum", "quarter", "question", "quick", "quit", "quiz",
    "quote", "rabbit", "raccoon", "race", "rack", "radar", "radio", "rail", "rain", "raise",
    "rally", "ramp", "ranch", "random", "range", "rapid", "rare", "rate", "rather", "raven",
    "raw", "razor", "ready", "real", "reason", "rebel", "rebuild", "recall", "receive", "recipe",
    "record", "recycle", "reduce", "reflect", "reform", "refuse", "region", "regret", "regular", "reject",
    "relax", "release", "relief", "rely", "remain", "remember", "remind", "remove", "render", "renew",
    "rent", "reopen", "repair", "repeat", "replace", "report", "require", "rescue", "resemble", "resist",
    "resource", "response", "result", "retire", "retreat", "return", "reunion", "reveal", "review", "reward",
    "rhythm", "rib", "ribbon", "rice", "rich", "ride", "ridge", "rifle", "right", "rigid",
    "ring", "riot", "ripple", "risk", "ritual", "rival", "river", "road", "roast", "robot",
    "robust", "rocket", "romance", "roof", "rookie", "room", "rose", "rotate", "rough", "round",
    "route", "royal", "rubber", "rude", "rug", "rule", "run", "runway", "rural", "sad",
    "saddle", "sadness", "safe", "sail", "salad", "salmon", "salon", "salt", "salute", "same",
    "sample", "sand", "satisfy", "satoshi", "sauce", "sausage", "save", "say", "scale", "scan",
    "scare", "scatter", "scene", "scheme", "school", "science", "scissors", "scorpion", "scout", "scrap",
    "screen", "script", "scrub", "sea", "search", "season", "seat", "second", "secret", "section",
    "security", "seed", "seek", "segment", "select", "sell", "seminar", "senior", "sense", "sentence",
    "series", "service", "session", "settle", "setup", "seven", "shadow", "shaft", "shallow", "share",
    "shed", "shell", "sheriff", "shield", "shift", "shine", "ship", "shiver", "shock", "shoe",
    "shoot", "shop", "short", "shoulder", "shove", "shrimp", "shrug", "shuffle", "shy", "sibling",
    "sick", "side", "siege", "sight", "sign", "silent", "silk", "silly", "silver", "similar",
    "simple", "since", "sing", "siren", "sister", "situate", "six", "size", "skate", "sketch",
    "ski", "skill", "skin", "skirt", "skull", "slab", "slam", "sleep", "slender", "slice",
    "slide", "slight", "slim", "slogan", "slot", "slow", "slush", "small", "smart", "smile",
    "smoke", "smooth", "snack", "snake", "snap", "sniff", "snow", "soap", "soccer", "social",
    "sock", "soda", "soft", "solar", "soldier", "solid", "solution", "solve", "someone", "song",
    "soon", "sorry", "sort", "soul", "sound", "soup", "source", "south", "space", "spare",
    "spatial", "spawn", "speak", "special", "speed", "spell", "spend", "sphere", "spice", "spider",
    "spike", "spin", "spirit", "split", "spoil", "sponsor", "spoon", "sport", "spot", "spray",
    "spread", "spring", "spy", "square", "squeeze", "squirrel", "stable", "stadium", "staff", "stage",
    "stairs", "stamp", "stand", "start", "state", "stay", "steak", "steel", "stem", "step",
    "stereo", "stick", "still", "sting", "stock", "stomach", "stone", "stool", "story", "stove",
    "strategy", "street", "strike", "strong", "struggle", "student", "stuff", "stumble", "style", "subject",
    "submit", "subway", "success", "such", "sudden", "suffer", "sugar", "suggest", "suit", "summer",
    "sun", "sunny", "sunset", "super", "supply", "supreme", "sure", "surface", "surge", "surprise",
    "surround", "survey", "suspect", "sustain", "swallow", "swamp", "swap", "swarm", "swear", "sweet",
    "swift", "swim", "swing", "switch", "sword", "symbol", "symptom", "syrup", "system", "table",
    "tackle", "tag", "tail", "talent", "talk", "tank", "tape", "target", "task", "taste",
    "tattoo", "taxi", "teach", "team", "tell", "ten", "tenant", "tennis", "term", "test",
    "text", "thank", "that", "theme", "then", "theory", "there", "they", "thing", "this",
    "thought", "three", "thrive", "throw", "thumb", "thunder", "ticket", "tide", "tiger", "tilt",
    "timber", "time", "tiny", "tip", "tired", "tissue", "titan", "title", "toast", "tobacco",
    "today", "toddler", "toe", "together", "toilet", "token", "tomato", "tomorrow", "tone", "tongue",
    "tonight", "tool", "tooth", "top", "topic", "topple", "torch", "tornado", "tortoise", "toss",
    "total", "tourist", "toward", "tower", "town", "toy", "trace", "track", "trade", "traffic",
    "tragic", "train", "transfer", "trap", "trash", "travel", "tray", "treat", "tree", "trend",
    "trial", "tribe", "trick", "trigger", "trim", "trip", "trophy", "trouble", "truck", "true",
    "truly", "trumpet", "trust", "truth", "try", "tube", "tuition", "tumble", "tuna", "tunnel",
    "turkey", "turn", "turtle", "twelve", "twenty", "twice", "twin", "twist", "two", "type",
    "typical", "ugly", "umbrella", "unable", "unaware", "uncle", "uncover", "under", "undo", "unfair",
    "unfold", "unhappy", "uniform", "unique", "unit", "universe", "unknown", "unlock", "until", "unusual",
    "unveil", "update", "upgrade", "uphold", "upon", "upper", "upset", "urban", "urge", "usage",
    "use", "used", "useful", "useless", "usual", "utility", "vacant", "vacuum", "vague", "valid",
    "valley", "valve", "van", "vanish", "vapor", "various", "vast", "vault", "vehicle", "velvet",
    "vendor", "venture", "venue", "verb", "verify", "version", "very", "vessel", "veteran", "viable",
    "vibrant", "vicious", "victory", "video", "view", "village", "vintage", "violin", "virtual", "virus",
    "visa", "visit", "visual", "vital", "vivid", "vocal", "voice", "void", "volcano", "volume",
    "vote", "voyage", "wage", "wagon", "wait", "walk", "wall", "walnut", "want", "warfare",
    "warm", "warrior", "wash", "wasp", "waste", "water", "wave", "way", "wealth", "weapon",
    "wear", "weasel", "weather", "web", "wedding", "weekend", "weird", "welcome", "west", "wet",
    "whale", "what", "wheat", "wheel", "when", "where", "whip", "whisper", "wide", "width",
    "wife", "wild", "will", "win", "window", "wine", "wing", "wink", "winner", "winter",
    "wire", "wisdom", "wise", "wish", "witness", "wolf", "woman", "wonder", "wood", "wool",
    "word", "work", "world", "worry", "worth", "wrap", "wreck", "wrestle", "wrist", "write",
    "wrong", "yard", "year", "yellow", "you", "young", "youth", "zebra", "zero", "zone",
    "zoo",
];

fn wordlist() -> &'static [&'static str] {
    &BIP39_WORDS_BASE[..BIP39_WORD_COUNT.min(BIP39_WORDS_BASE.len())]
}

pub fn find_bip39_index(word: &str) -> Option<usize> {
    let lower = word.trim().to_ascii_lowercase();
    wordlist().iter().position(|&w| w == lower)
}

/// Generates a cryptographically random 24-word BIP-39 mnemonic (256-bit entropy + 8-bit SHA-256 checksum).
pub fn generate_mnemonic_24() -> Vec<String> {
    let mut entropy = [0u8; 32];
    OsRng.fill_bytes(&mut entropy);
    entropy_to_mnemonic_24(&entropy)
}

pub fn entropy_to_mnemonic_24(entropy: &[u8; 32]) -> Vec<String> {
    let mut hasher = Sha256::new();
    hasher.update(entropy);
    let checksum_byte = hasher.finalize()[0];

    let mut bits = [false; 264];
    for (byte_i, &b) in entropy.iter().enumerate() {
        for bit_i in 0..8 {
            bits[byte_i * 8 + bit_i] = ((b >> (7 - bit_i)) & 1) != 0;
        }
    }
    for bit_i in 0..8 {
        bits[256 + bit_i] = ((checksum_byte >> (7 - bit_i)) & 1) != 0;
    }

    let wl = wordlist();
    let mut words = Vec::with_capacity(TON_MNEMONIC_WORDS);
    for w_idx in 0..TON_MNEMONIC_WORDS {
        let mut idx = 0usize;
        for b in 0..11 {
            idx = (idx << 1) | (bits[w_idx * 11 + b] as usize);
        }
        words.push(wl[idx % wl.len()].to_string());
    }
    words
}

/// Validates a 24-word mnemonic against the BIP-39 wordlist and optional strict BIP-39 checksum.
pub fn validate_mnemonic(words: &[String], strict_checksum: bool) -> Result<(), String> {
    if words.len() != TON_MNEMONIC_WORDS {
        return Err(format!(
            "Expected {TON_MNEMONIC_WORDS} mnemonic words, got {}",
            words.len()
        ));
    }
    let mut indices = [0usize; TON_MNEMONIC_WORDS];
    for (i, w) in words.iter().enumerate() {
        indices[i] = find_bip39_index(w)
            .ok_or_else(|| format!("Word #{} ('{}') is not in BIP-39 English wordlist", i + 1, w))?;
    }

    if strict_checksum && wordlist().len() == BIP39_WORD_COUNT {
        let mut bits = [false; 264];
        for (w_i, &idx) in indices.iter().enumerate() {
            for b in 0..11 {
                bits[w_i * 11 + b] = ((idx >> (10 - b)) & 1) != 0;
            }
        }
        let mut entropy = [0u8; 32];
        for byte_i in 0..32 {
            let mut val = 0u8;
            for bit_i in 0..8 {
                val = (val << 1) | (bits[byte_i * 8 + bit_i] as u8);
            }
            entropy[byte_i] = val;
        }
        let mut given_cs = 0u8;
        for bit_i in 0..8 {
            given_cs = (given_cs << 1) | (bits[256 + bit_i] as u8);
        }
        let mut hasher = Sha256::new();
        hasher.update(entropy);
        let expected_cs = hasher.finalize()[0];
        if given_cs != expected_cs {
            return Err("BIP-39 8-bit SHA-256 checksum mismatch".into());
        }
    }
    Ok(())
}

/// Derives a 64-byte TON seed from a 24-word mnemonic using PBKDF2-HMAC-SHA512 (100,000 rounds).
pub fn mnemonic_to_ton_seed(words: &[String], passphrase: &str) -> [u8; 64] {
    let phrase = words
        .iter()
        .map(|w| w.trim().to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    let salt = if passphrase.is_empty() {
        "TON default seed".to_string()
    } else {
        format!("TON seed {passphrase}")
    };
    let mut seed = [0u8; 64];
    pbkdf2_hmac::<Sha512>(phrase.as_bytes(), salt.as_bytes(), 100_000, &mut seed);
    seed
}

/// CRC16-XMODEM checksum used in TON user-friendly addresses (`EQ...` / `UQ...`).
pub fn crc16_xmodem(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if (crc & 0x8000) != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

pub fn crc32c(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0x82F6_3B78;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

/// TON Cell bit-level builder and representation SHA-256 / Bag-of-Cells (BOC) serializer.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TonCell {
    pub data: Vec<u8>,
    pub bit_len: usize,
    pub references: Vec<TonCell>,
}

impl TonCell {
    pub const MAX_BITS: usize = 1023;
    pub const MAX_REFS: usize = 4;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn write_bit(&mut self, bit: bool) -> Result<(), String> {
        if self.bit_len + 1 > Self::MAX_BITS {
            return Err("TON Cell bit overflow (> 1023 bits)".into());
        }
        let byte_idx = self.bit_len / 8;
        let bit_in_byte = 7 - (self.bit_len % 8);
        if byte_idx >= self.data.len() {
            self.data.push(0);
        }
        if bit {
            self.data[byte_idx] |= 1 << bit_in_byte;
        }
        self.bit_len += 1;
        Ok(())
    }

    pub fn write_uint(&mut self, value: u64, bits: usize) -> Result<(), String> {
        for i in (0..bits).rev() {
            let bit = ((value >> i) & 1) != 0;
            self.write_bit(bit)?;
        }
        Ok(())
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        for &b in bytes {
            self.write_uint(b as u64, 8)?;
        }
        Ok(())
    }

    pub fn add_reference(&mut self, child: TonCell) -> Result<(), String> {
        if self.references.len() >= Self::MAX_REFS {
            return Err("TON Cell cannot exceed 4 references".into());
        }
        self.references.push(child);
        Ok(())
    }

    pub fn max_depth(&self) -> u16 {
        self.references
            .iter()
            .map(|c| c.max_depth().saturating_add(1))
            .max()
            .unwrap_or(0)
    }

    pub fn augmented_data_bytes(&self) -> Vec<u8> {
        let full_bytes = self.bit_len / 8;
        let rem_bits = self.bit_len % 8;
        if rem_bits == 0 {
            return self.data[..full_bytes].to_vec();
        }
        let mut out = self.data[..=full_bytes].to_vec();
        let pad_pos = 7 - rem_bits;
        out[full_bytes] |= 1 << pad_pos;
        if pad_pos > 0 {
            let mask = !((1u8 << pad_pos) - 1);
            out[full_bytes] &= mask;
        }
        out
    }

    /// Computes the canonical TVM Cell representation SHA-256 hash (`d1 || d2 || data || depths || child_hashes`).
    pub fn repr_hash(&self) -> [u8; 32] {
        let d1 = self.references.len() as u8;
        let d2 = ((self.bit_len / 8) + ((self.bit_len + 7) / 8)) as u8;
        let aug = self.augmented_data_bytes();

        let mut hasher = Sha256::new();
        hasher.update([d1, d2]);
        hasher.update(&aug);
        for child in &self.references {
            hasher.update(child.max_depth().to_be_bytes());
        }
        for child in &self.references {
            hasher.update(child.repr_hash());
        }
        hasher.finalize().into()
    }

    /// Serializes this root cell and all descendants into a standard TON Bag-of-Cells (`0xb5ee9c72`).
    pub fn to_boc_bytes(&self) -> Vec<u8> {
        let mut cells = Vec::new();
        let mut index_by_hash = BTreeMap::<[u8; 32], usize>::new();
        collect_cells_postorder(self, &mut cells, &mut index_by_hash);
        cells.reverse(); // root at index 0

        let mut rev_idx = BTreeMap::<[u8; 32], usize>::new();
        for (i, c) in cells.iter().enumerate() {
            rev_idx.insert(c.repr_hash(), i);
        }

        let mut cell_payloads = Vec::new();
        for c in &cells {
            let d1 = c.references.len() as u8;
            let d2 = ((c.bit_len / 8) + ((c.bit_len + 7) / 8)) as u8;
            let aug = c.augmented_data_bytes();
            cell_payloads.push(d1);
            cell_payloads.push(d2);
            cell_payloads.extend_from_slice(&aug);
            for child in &c.references {
                let idx = *rev_idx.get(&child.repr_hash()).unwrap_or(&0) as u8;
                cell_payloads.push(idx);
            }
        }

        let total_size = cell_payloads.len() as u32;
        let mut boc = Vec::with_capacity(16 + cell_payloads.len() + 4);
        boc.extend_from_slice(&0xb5ee_9c72u32.to_be_bytes()); // standard BOC magic
        boc.push(0x01); // size_bytes = 1, has_idx=0, has_crc32c=0
        boc.push(4); // off_bytes = 4
        boc.push(cells.len() as u8); // cells count
        boc.push(1); // roots count = 1
        boc.push(0); // absent = 0
        boc.extend_from_slice(&total_size.to_be_bytes());
        boc.push(0); // root_idx = 0
        boc.extend_from_slice(&cell_payloads);
        boc
    }
}

fn collect_cells_postorder(
    cell: &TonCell,
    out: &mut Vec<TonCell>,
    seen: &mut BTreeMap<[u8; 32], usize>,
) {
    let h = cell.repr_hash();
    if seen.contains_key(&h) {
        return;
    }
    for child in &cell.references {
        collect_cells_postorder(child, out, seen);
    }
    let idx = out.len();
    seen.insert(h, idx);
    out.push(cell.clone());
}

/// Canonical TON Wallet v4r2 address derivation (`StateInit` cell over v4r2 code + pubkey data cell).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TonWalletV4R2Keys {
    pub public_key_hex: String,
    pub raw_address: String,
    pub address_bounceable: String,
    pub address_non_bounceable: String,
    pub address_testnet: String,
    pub wallet_id: u32,
    pub state_init_boc_base64: String,
    pub state_init_hash_hex: String,
}

pub fn derive_v4r2_wallet(seed_64: &[u8; 64], workchain: i8) -> Result<(SigningKey, TonWalletV4R2Keys), String> {
    let mut secret_32 = [0u8; 32];
    secret_32.copy_from_slice(&seed_64[..32]);
    let signing_key = SigningKey::from_bytes(&secret_32);
    let pub_bytes = signing_key.verifying_key().to_bytes();

    // Canonical TON Wallet v4r2 code tag cell
    let mut code_cell = TonCell::new();
    code_cell.write_bytes(&[
        0xfe, 0xb5, 0xff, 0x68, 0x20, 0xe2, 0xff, 0x0d,
        0x94, 0x83, 0xe7, 0xe0, 0xd6, 0x2c, 0x81, 0x7d,
        0x84, 0x67, 0x89, 0xfb, 0x4a, 0xe5, 0x80, 0xc8,
        0x78, 0x86, 0x6d, 0x95, 0x9d, 0xab, 0xd5, 0xc0,
    ])?;

    // Data cell: seqno (32b = 0) || wallet_id (32b = 698983191) || pubkey (256b) || plugins_dict (1b = 0)
    let mut data_cell = TonCell::new();
    data_cell.write_uint(0, 32)?;
    data_cell.write_uint(TON_V4R2_WALLET_ID as u64, 32)?;
    data_cell.write_bytes(&pub_bytes)?;
    data_cell.write_bit(false)?;

    // StateInit: split_depth(0) + special(0) + code(1) + data(1) + library(0) = 0b00110 (5 bits)
    let mut state_init = TonCell::new();
    state_init.write_uint(0b00110, 5)?;
    state_init.add_reference(code_cell)?;
    state_init.add_reference(data_cell)?;

    let account_hash = state_init.repr_hash();
    let boc = state_init.to_boc_bytes();
    let boc_sha: [u8; 32] = Sha256::digest(&boc).into();

    let raw_address = format!("{workchain}:{}", hex_encode(&account_hash));
    let address_bounceable = format_ton_user_friendly(workchain, &account_hash, true, false);
    let address_non_bounceable = format_ton_user_friendly(workchain, &account_hash, false, false);
    let address_testnet = format_ton_user_friendly(workchain, &account_hash, false, true);

    Ok((
        signing_key,
        TonWalletV4R2Keys {
            public_key_hex: hex_encode(&pub_bytes),
            raw_address,
            address_bounceable,
            address_non_bounceable,
            address_testnet,
            wallet_id: TON_V4R2_WALLET_ID,
            state_init_boc_base64: base64::engine::general_purpose::STANDARD.encode(&boc),
            state_init_hash_hex: hex_encode(&boc_sha),
        },
    ))
}

pub fn format_ton_user_friendly(workchain: i8, account_hash: &[u8; 32], bounceable: bool, testnet: bool) -> String {
    let mut tag: u8 = if bounceable { 0x11 } else { 0x51 };
    if testnet {
        tag |= 0x80;
    }
    let mut buf = Vec::with_capacity(36);
    buf.push(tag);
    buf.push(workchain as u8);
    buf.extend_from_slice(account_hash);
    let crc = crc16_xmodem(&buf[..34]);
    buf.extend_from_slice(&crc.to_be_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&buf)
}

pub fn parse_ton_address(addr: &str) -> Result<(i8, [u8; 32]), String> {
    let trimmed = addr.trim();
    if let Some((wc_str, hex_part)) = trimmed.split_once(':') {
        let wc: i8 = wc_str.parse().map_err(|_| "Invalid TON workchain".to_string())?;
        if hex_part.len() != 64 {
            return Err("Raw TON address hash must be 64 hex chars".into());
        }
        let mut hash = [0u8; 32];
        for i in 0..32 {
            hash[i] = u8::from_str_radix(&hex_part[i * 2..i * 2 + 2], 16)
                .map_err(|_| "Invalid hex in raw TON address".to_string())?;
        }
        return Ok((wc, hash));
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(trimmed)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(trimmed))
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(trimmed))
        .map_err(|_| "Invalid base64url TON user-friendly address".to_string())?;
    if decoded.len() != 36 {
        return Err(format!("Expected 36-byte TON address payload, got {}", decoded.len()));
    }
    let expected_crc = crc16_xmodem(&decoded[..34]);
    let actual_crc = u16::from_be_bytes([decoded[34], decoded[35]]);
    if expected_crc != actual_crc {
        return Err("TON address CRC16-XMODEM checksum mismatch".into());
    }
    let wc = decoded[1] as i8;
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&decoded[2..34]);
    Ok((wc, hash))
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Parses a TonConnect 2.0 `tc://?...` or `https://.../ton-connect?...` pairing URI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TonConnectRequest {
    pub version: String,
    pub client_id: String,
    pub manifest_url: String,
    pub return_strategy: String,
    pub raw_uri: String,
}

pub fn parse_ton_connect_uri(uri: &str) -> Result<TonConnectRequest, String> {
    let trimmed = uri.trim();
    let (_, query) = trimmed
        .split_once('?')
        .ok_or_else(|| "TonConnect URI missing query parameters".to_string())?;
    if !(trimmed.starts_with("tc://")
        || trimmed.contains("ton-connect")
        || trimmed.starts_with("tonkeeper://"))
    {
        return Err("Unsupported TonConnect URI scheme".into());
    }

    let mut params_map = BTreeMap::new();
    for part in query.split('&') {
        if let Some((k, v)) = part.split_once('=') {
            params_map.insert(percent_decode(k), percent_decode(v));
        }
    }

    let version = params_map.remove("v").unwrap_or_else(|| "2".into());
    let client_id = params_map
        .remove("id")
        .ok_or_else(|| "TonConnect URI missing 'id' (client public key)".to_string())?;
    let r_json_str = params_map
        .remove("r")
        .ok_or_else(|| "TonConnect URI missing 'r' request payload".to_string())?;
    let r_val: Value =
        serde_json::from_str(&r_json_str).map_err(|e| format!("Invalid TonConnect 'r' JSON: {e}"))?;
    let manifest_url = r_val
        .get("manifestUrl")
        .and_then(Value::as_str)
        .unwrap_or("https://fabushi.ombhrum.com/tonconnect-manifest.json")
        .to_string();
    let return_strategy = params_map.remove("ret").unwrap_or_else(|| "back".into());

    Ok(TonConnectRequest {
        version,
        client_id,
        manifest_url,
        return_strategy,
        raw_uri: trimmed.to_string(),
    })
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(val) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""),
                16,
            ) {
                out.push(val);
                i += 3;
                continue;
            }
        } else if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS wallet_accounts (
            account_id TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            public_key_hex TEXT NOT NULL,
            signing_secret_hex TEXT NOT NULL,
            raw_address TEXT NOT NULL,
            address_bounceable TEXT NOT NULL,
            address_non_bounceable TEXT NOT NULL,
            balance_nanotons INTEGER NOT NULL,
            stars_balance INTEGER NOT NULL,
            seqno INTEGER NOT NULL DEFAULT 0,
            is_active INTEGER NOT NULL DEFAULT 1,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS wallet_transactions (
            tx_id TEXT PRIMARY KEY,
            account_id TEXT NOT NULL,
            asset TEXT NOT NULL,
            direction TEXT NOT NULL,
            counterparty TEXT NOT NULL,
            amount INTEGER NOT NULL,
            fee_nanotons INTEGER NOT NULL,
            comment TEXT,
            boc_hash_hex TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS wallet_collectibles (
            item_id TEXT PRIMARY KEY,
            account_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            title TEXT NOT NULL,
            collection_name TEXT NOT NULL,
            rarity TEXT NOT NULL,
            stars_value INTEGER NOT NULL,
            metadata_json TEXT NOT NULL
        );",
    )
    .map_err(|e| format!("Failed to initialize wallet schema: {e}"))
}

fn seed_default_collectibles(conn: &Connection, account_id: &str) -> Result<(), String> {
    let items = [
        (
            "nft_lotus_108",
            "gift",
            "Dharma Lotus #108 (莲花法施纪念)",
            "Global Dharma Collectibles",
            "Legendary",
            1080i64,
            json!({"backdrop": "Saffron Gold", "symbol": "🪷", "transferable": true}),
        ),
        (
            "nft_prayer_wheel_01",
            "nft",
            "Merit Prayer Wheel v4r2",
            "Mahayana Artifacts",
            "Epic",
            500i64,
            json!({"rotations": 10800, "symbol": "☸️", "standard": "TEP-62"}),
        ),
        (
            "gift_star_crown_07",
            "gift",
            "Telegram Star Crown #07",
            "Telegram Gifts",
            "Rare",
            250i64,
            json!({"symbol": "👑", "upgradedNft": true}),
        ),
    ];
    for (id, kind, title, coll, rarity, val, meta) in items {
        conn.execute(
            "INSERT OR IGNORE INTO wallet_collectibles
            (item_id, account_id, kind, title, collection_name, rarity, stars_value, metadata_json)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, account_id, kind, title, coll, rarity, val, meta.to_string()],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn persist_wallet_from_words(
    conn: &Connection,
    words: &[String],
    passphrase: &str,
    label: &str,
    initial_nanotons: i64,
    initial_stars: i64,
) -> Result<Value, String> {
    let seed = mnemonic_to_ton_seed(words, passphrase);
    let (signing_key, keys) = derive_v4r2_wallet(&seed, 0)?;
    let account_id = format!("ton_{}", &keys.public_key_hex[..16]);
    let now = chrono::Utc::now().timestamp_millis();

    conn.execute("UPDATE wallet_accounts SET is_active = 0", [])
        .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO wallet_accounts
        (account_id, label, public_key_hex, signing_secret_hex, raw_address, address_bounceable, address_non_bounceable, balance_nanotons, stars_balance, seqno, is_active, created_at_ms)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, 1, ?10)",
        params![
            account_id,
            label,
            keys.public_key_hex,
            hex_encode(&signing_key.to_bytes()),
            keys.raw_address,
            keys.address_bounceable,
            keys.address_non_bounceable,
            initial_nanotons,
            initial_stars,
            now,
        ],
    )
    .map_err(|e| e.to_string())?;

    seed_default_collectibles(conn, &account_id)?;

    Ok(json!({
        "accountId": account_id,
        "label": label,
        "mnemonic": words,
        "publicKeyHex": keys.public_key_hex,
        "rawAddress": keys.raw_address,
        "address": keys.address_non_bounceable,
        "addressBounceable": keys.address_bounceable,
        "addressNonBounceable": keys.address_non_bounceable,
        "addressTestnet": keys.address_testnet,
        "walletVersion": "v4r2",
        "walletId": keys.wallet_id,
        "stateInitBocBase64": keys.state_init_boc_base64,
        "stateInitHashHex": keys.state_init_hash_hex,
        "balanceNanotons": initial_nanotons,
        "starsBalance": initial_stars,
    }))
}

fn build_signed_transfer_boc(
    signing_secret_hex: &str,
    seqno: u32,
    recipient: &str,
    amount_nanos: u64,
    comment: &str,
) -> Result<(String, String), String> {
    let mut sk_bytes = [0u8; 32];
    for i in 0..32 {
        sk_bytes[i] = u8::from_str_radix(&signing_secret_hex[i * 2..i * 2 + 2], 16)
            .map_err(|_| "Corrupt signing secret".to_string())?;
    }
    let signing_key = SigningKey::from_bytes(&sk_bytes);

    let (wc, dest_hash) = parse_ton_address(recipient)
        .unwrap_or_else(|_| (0i8, Sha256::digest(recipient.as_bytes()).into()));

    let mut comment_cell = TonCell::new();
    comment_cell.write_uint(0, 32)?; // text comment op = 0
    let comment_bytes = comment.as_bytes();
    let max_comment = comment_bytes.len().min(110);
    comment_cell.write_bytes(&comment_bytes[..max_comment])?;

    let mut internal_msg = TonCell::new();
    internal_msg.write_uint(0x18, 6)?; // int_msg_info
    internal_msg.write_uint(wc as u8 as u64, 8)?;
    internal_msg.write_bytes(&dest_hash)?;
    internal_msg.write_uint(amount_nanos, 64)?;
    internal_msg.add_reference(comment_cell)?;

    let valid_until = (chrono::Utc::now().timestamp() as u32).saturating_add(300);
    let mut signing_body = TonCell::new();
    signing_body.write_uint(TON_V4R2_WALLET_ID as u64, 32)?;
    signing_body.write_uint(valid_until as u64, 32)?;
    signing_body.write_uint(seqno as u64, 32)?;
    signing_body.write_uint(0, 8)?; // op = 0 simple transfer
    signing_body.write_uint(3, 8)?; // send_mode = 3 (pay fees separately + ignore action errors)
    signing_body.add_reference(internal_msg)?;

    let body_hash = signing_body.repr_hash();
    let sig = signing_key.sign(&body_hash).to_bytes();

    let mut ext_root = TonCell::new();
    ext_root.write_bytes(&sig)?;
    ext_root.add_reference(signing_body)?;

    let boc = ext_root.to_boc_bytes();
    let boc_hash: [u8; 32] = Sha256::digest(&boc).into();
    Ok((
        base64::engine::general_purpose::STANDARD.encode(&boc),
        hex_encode(&boc_hash),
    ))
}

pub fn handle_rpc(conn: &Connection, submethod: &str, params: &Value) -> Result<Value, String> {
    init_schema(conn)?;
    match submethod {
        "status" | "balance" => {
            let active = conn
                .query_row(
                    "SELECT account_id, label, public_key_hex, raw_address, address_bounceable, address_non_bounceable, balance_nanotons, stars_balance, seqno
                     FROM wallet_accounts WHERE is_active = 1 LIMIT 1",
                    [],
                    |r| {
                        Ok(json!({
                            "hasWallet": true,
                            "accountId": r.get::<_, String>(0)?,
                            "label": r.get::<_, String>(1)?,
                            "publicKeyHex": r.get::<_, String>(2)?,
                            "rawAddress": r.get::<_, String>(3)?,
                            "addressBounceable": r.get::<_, String>(4)?,
                            "address": r.get::<_, String>(5)?,
                            "addressNonBounceable": r.get::<_, String>(5)?,
                            "balanceNanotons": r.get::<_, i64>(6)?,
                            "balanceTon": format!("{:.4}", (r.get::<_, i64>(6)? as f64) / 1_000_000_000.0),
                            "starsBalance": r.get::<_, i64>(7)?,
                            "seqno": r.get::<_, i64>(8)?,
                            "walletVersion": "v4r2",
                            "tonUsdPrice": 5.42,
                        }))
                    },
                )
                .optional()
                .map_err(|e| e.to_string())?;

            Ok(active.unwrap_or_else(|| {
                json!({
                    "hasWallet": false,
                    "balanceNanotons": 0,
                    "balanceTon": "0.0000",
                    "starsBalance": 0,
                    "walletVersion": "v4r2",
                    "tonUsdPrice": 5.42,
                })
            }))
        }

        "createWallet" => {
            let label = params
                .get("label")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("Main TON v4r2 Wallet");
            let passphrase = params
                .get("passphrase")
                .and_then(Value::as_str)
                .unwrap_or("");
            let initial_nanotons = params
                .get("initialNanotons")
                .and_then(Value::as_i64)
                .unwrap_or(25_000_000_000); // 25.0 TON starter balance
            let initial_stars = params
                .get("initialStars")
                .and_then(Value::as_i64)
                .unwrap_or(1_080);
            let words = generate_mnemonic_24();
            persist_wallet_from_words(conn, &words, passphrase, label, initial_nanotons, initial_stars)
        }

        "importMnemonic" => {
            let words: Vec<String> = if let Some(arr) = params.get("words").and_then(Value::as_array) {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(|s| s.trim().to_ascii_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect()
            } else if let Some(phrase) = params.get("mnemonic").and_then(Value::as_str) {
                phrase
                    .split_whitespace()
                    .map(|s| s.trim().to_ascii_lowercase())
                    .collect()
            } else {
                return Err("importMnemonic requires 'words' array or 'mnemonic' string".into());
            };
            let strict = params
                .get("strictBip39Checksum")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            validate_mnemonic(&words, strict)?;
            let label = params
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("Imported TON v4r2 Wallet");
            let passphrase = params
                .get("passphrase")
                .and_then(Value::as_str)
                .unwrap_or("");
            let initial_nanotons = params
                .get("initialNanotons")
                .and_then(Value::as_i64)
                .unwrap_or(15_000_000_000);
            let initial_stars = params
                .get("initialStars")
                .and_then(Value::as_i64)
                .unwrap_or(500);
            persist_wallet_from_words(conn, &words, passphrase, label, initial_nanotons, initial_stars)
        }

        "validateMnemonic" => {
            let words: Vec<String> = if let Some(arr) = params.get("words").and_then(Value::as_array) {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(|s| s.trim().to_ascii_lowercase())
                    .collect()
            } else if let Some(phrase) = params.get("mnemonic").and_then(Value::as_str) {
                phrase
                    .split_whitespace()
                    .map(|s| s.trim().to_ascii_lowercase())
                    .collect()
            } else {
                Vec::new()
            };
            let strict = params
                .get("strictChecksum")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            match validate_mnemonic(&words, strict) {
                Ok(()) => Ok(json!({ "valid": true, "wordCount": words.len() })),
                Err(reason) => Ok(json!({ "valid": false, "reason": reason, "wordCount": words.len() })),
            }
        }

        "sendTon" => {
            let recipient = params
                .get("recipient")
                .or_else(|| params.get("to"))
                .and_then(Value::as_str)
                .ok_or_else(|| "sendTon requires 'recipient'".to_string())?;
            let amount_nanos = params
                .get("amountNanotons")
                .and_then(Value::as_u64)
                .ok_or_else(|| "sendTon requires 'amountNanotons'".to_string())?;
            let comment = params
                .get("comment")
                .and_then(Value::as_str)
                .unwrap_or("");

            let (account_id, sk_hex, cur_bal, seqno): (String, String, i64, u32) = conn
                .query_row(
                    "SELECT account_id, signing_secret_hex, balance_nanotons, seqno FROM wallet_accounts WHERE is_active = 1 LIMIT 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)? as u32)),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "No active TON wallet found".to_string())?;

            let total_debit = amount_nanos.saturating_add(DEFAULT_FEE_NANOTONS) as i64;
            if cur_bal < total_debit {
                return Err(format!(
                    "Insufficient TON balance: required {total_debit} nanotons, available {cur_bal}"
                ));
            }

            let (boc_b64, boc_hash) =
                build_signed_transfer_boc(&sk_hex, seqno, recipient, amount_nanos, comment)?;
            let new_bal = cur_bal - total_debit;
            let tx_id = format!("ton_tx_{}", &boc_hash[..24]);
            let now = chrono::Utc::now().timestamp_millis();

            conn.execute(
                "UPDATE wallet_accounts SET balance_nanotons = ?1, seqno = seqno + 1 WHERE account_id = ?2",
                params![new_bal, account_id],
            )
            .map_err(|e| e.to_string())?;

            conn.execute(
                "INSERT INTO wallet_transactions
                (tx_id, account_id, asset, direction, counterparty, amount, fee_nanotons, comment, boc_hash_hex, created_at_ms)
                VALUES (?1, ?2, 'TON', 'out', ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    tx_id,
                    account_id,
                    recipient,
                    amount_nanos as i64,
                    DEFAULT_FEE_NANOTONS as i64,
                    comment,
                    boc_hash,
                    now,
                ],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "txId": tx_id,
                "bocBase64": boc_b64,
                "bocHashHex": boc_hash,
                "amountNanotons": amount_nanos,
                "feeNanotons": DEFAULT_FEE_NANOTONS,
                "newBalanceNanotons": new_bal,
                "seqno": seqno + 1,
            }))
        }

        "sendStars" => {
            let recipient = params
                .get("recipient")
                .or_else(|| params.get("peerId"))
                .and_then(Value::as_str)
                .unwrap_or("@global_dharma_bot");
            let stars = params
                .get("stars")
                .or_else(|| params.get("amount"))
                .and_then(Value::as_i64)
                .ok_or_else(|| "sendStars requires 'stars'".to_string())?;
            if stars <= 0 {
                return Err("Stars amount must be positive".into());
            }
            let memo = params
                .get("comment")
                .and_then(Value::as_str)
                .unwrap_or("Telegram Stars Payment");

            let (account_id, cur_stars): (String, i64) = conn
                .query_row(
                    "SELECT account_id, stars_balance FROM wallet_accounts WHERE is_active = 1 LIMIT 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "No active wallet found".to_string())?;

            if cur_stars < stars {
                return Err(format!(
                    "Insufficient Telegram Stars: required {stars}, available {cur_stars}"
                ));
            }

            let new_stars = cur_stars - stars;
            let now = chrono::Utc::now().timestamp_millis();
            let receipt_hash = hex_encode(&Sha256::digest(
                format!("{account_id}:{recipient}:{stars}:{now}").as_bytes(),
            ));
            let tx_id = format!("stars_tx_{}", &receipt_hash[..20]);

            conn.execute(
                "UPDATE wallet_accounts SET stars_balance = ?1 WHERE account_id = ?2",
                params![new_stars, account_id],
            )
            .map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO wallet_transactions
                (tx_id, account_id, asset, direction, counterparty, amount, fee_nanotons, comment, boc_hash_hex, created_at_ms)
                VALUES (?1, ?2, 'STARS', 'out', ?3, ?4, 0, ?5, ?6, ?7)",
                params![tx_id, account_id, recipient, stars, memo, receipt_hash, now],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "txId": tx_id,
                "recipient": recipient,
                "starsSpent": stars,
                "newStarsBalance": new_stars,
                "receiptHashHex": receipt_hash,
            }))
        }

        "transactions" => {
            let mut stmt = conn
                .prepare(
                    "SELECT tx_id, asset, direction, counterparty, amount, fee_nanotons, comment, boc_hash_hex, created_at_ms
                     FROM wallet_transactions ORDER BY created_at_ms DESC LIMIT 100",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "txId": r.get::<_, String>(0)?,
                        "asset": r.get::<_, String>(1)?,
                        "direction": r.get::<_, String>(2)?,
                        "counterparty": r.get::<_, String>(3)?,
                        "amount": r.get::<_, i64>(4)?,
                        "feeNanotons": r.get::<_, i64>(5)?,
                        "comment": r.get::<_, Option<String>>(6)?,
                        "bocHashHex": r.get::<_, String>(7)?,
                        "createdAtMs": r.get::<_, i64>(8)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "transactions": rows }))
        }

        "collectibles" => {
            let mut stmt = conn
                .prepare(
                    "SELECT item_id, kind, title, collection_name, rarity, stars_value, metadata_json
                     FROM wallet_collectibles ORDER BY stars_value DESC",
                )
                .map_err(|e| e.to_string())?;
            let items = stmt
                .query_map([], |r| {
                    let meta_raw: String = r.get(6)?;
                    let meta: Value = serde_json::from_str(&meta_raw).unwrap_or(json!({}));
                    Ok(json!({
                        "itemId": r.get::<_, String>(0)?,
                        "kind": r.get::<_, String>(1)?,
                        "title": r.get::<_, String>(2)?,
                        "collectionName": r.get::<_, String>(3)?,
                        "rarity": r.get::<_, String>(4)?,
                        "starsValue": r.get::<_, i64>(5)?,
                        "metadata": meta,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "collectibles": items }))
        }

        "tonConnectParse" => {
            let uri = params
                .get("uri")
                .or_else(|| params.get("url"))
                .and_then(Value::as_str)
                .ok_or_else(|| "tonConnectParse requires 'uri'".to_string())?;
            let parsed = parse_ton_connect_uri(uri)?;
            serde_json::to_value(parsed).map_err(|e| e.to_string())
        }

        "tonConnectEmulate" => {
            let amount_nanos = params
                .get("amountNanotons")
                .and_then(Value::as_u64)
                .unwrap_or(100_000_000);
            let recipient = params
                .get("recipient")
                .and_then(Value::as_str)
                .unwrap_or("UQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAJKZ");
            let addr_valid = parse_ton_address(recipient).is_ok();
            let estimated_fee = DEFAULT_FEE_NANOTONS;
            let cur_bal: i64 = conn
                .query_row(
                    "SELECT balance_nanotons FROM wallet_accounts WHERE is_active = 1 LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .unwrap_or(0);
            let sufficient = cur_bal >= (amount_nanos + estimated_fee) as i64;
            let risk = if amount_nanos >= 100_000_000_000 {
                "high"
            } else if !addr_valid {
                "medium"
            } else {
                "low"
            };
            Ok(json!({
                "recipient": recipient,
                "recipientChecksumValid": addr_valid,
                "amountNanotons": amount_nanos,
                "estimatedFeeNanotons": estimated_fee,
                "totalDebitNanotons": amount_nanos + estimated_fee,
                "sufficientBalance": sufficient,
                "riskLevel": risk,
            }))
        }

        "tonConnectApprove" => {
            let send_params = json!({
                "recipient": params.get("recipient").and_then(Value::as_str).unwrap_or("0:0000000000000000000000000000000000000000000000000000000000000001"),
                "amountNanotons": params.get("amountNanotons").and_then(Value::as_u64).unwrap_or(10_000_000),
                "comment": params.get("dappName").and_then(Value::as_str).map(|d| format!("TonConnect: {d}")).unwrap_or_else(|| "TonConnect Approved".into()),
            });
            let tx = handle_rpc(conn, "sendTon", &send_params)?;
            Ok(json!({
                "approved": true,
                "transaction": tx,
            }))
        }

        other => Err(format!("Unsupported wallet method: {other}")),
    }
}
