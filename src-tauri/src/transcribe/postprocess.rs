//! Post-processing pipeline applied after Whisper produces a transcript.
//!
//! Order (per plan §6 #11):
//!   1. Filler-word removal
//!   2. Voice-command substitution (period / comma / etc.)
//!   3. Auto-capitalization
//!   4. Auto-period
//!   5. Dictionary replacements
//!   6. Snippet expansion
//!
//! Every stage is gated by a flag in `Settings`. Replacements + snippets
//! come from the unified Dictionary (`db::DictionaryEntry`).

use std::collections::HashMap;

use regex::Regex;

use crate::db::DictionaryEntry;
use crate::settings::Settings;

#[derive(Debug, Default, Clone)]
pub struct ProcessOutcome {
    pub text: String,
    /// Map of stripped filler word (lowercased) → number of times removed.
    pub stripped_fillers: HashMap<String, i64>,
}

pub fn process(
    text: &str,
    settings: &Settings,
    dictionary: &[DictionaryEntry],
) -> ProcessOutcome {
    let mut stripped_fillers: HashMap<String, i64> = HashMap::new();

    // Literal mode: "type exactly what I said." Bypass the ENTIRE pipeline
    // (self-corrections, filler-strip, voice commands, capitalization, lists,
    // dictionary) so dialogue and prose land verbatim. This is the escape
    // hatch for writers whose words must not be silently rewritten.
    if settings.literal_mode {
        return ProcessOutcome {
            text: text.trim().to_string(),
            stripped_fillers,
        };
    }

    let mut out = text.to_string();

    // Self-corrections fire FIRST so the rest of the pipeline operates on
    // the already-fixed sentence. (Skipped entirely in literal mode above.)
    out = apply_self_corrections(&out);

    if settings.strip_fillers && !settings.filler_words.is_empty() {
        let (cleaned, counts) = strip_fillers_with_counts(&out, &settings.filler_words);
        out = cleaned;
        for (k, v) in counts {
            *stripped_fillers.entry(k).or_insert(0) += v;
        }
    }
    out = apply_voice_commands(&out, settings);
    if settings.auto_capitalize {
        out = auto_capitalize(&out);
    }
    if settings.auto_period {
        out = auto_period(&out);
    }
    // Numbered-list detection runs AFTER capitalization + period so it
    // sees the sentence-segmented form Whisper produced. Turning the
    // markers into actual `1.` `2.` form adds newlines that auto_period
    // would otherwise wreck.
    if settings.auto_numbered_lists {
        out = apply_numbered_lists(&out);
    }
    if settings.auto_bulleted_lists {
        out = apply_bulleted_lists(&out);
    }
    // Intelligently format unmarked spoken lists ("I need X, Y, and Z") that
    // the explicit numbered/bulleted passes above didn't catch.
    if settings.auto_smart_lists {
        out = apply_natural_lists(&out);
    }
    // Fuzzy proper-noun correction runs after list formatting and BEFORE
    // literal replacements, snapping near-miss tokens to the intended
    // dictionary "word" spelling (e.g. "Sersha" → "Saoirse").
    if settings.fuzzy_dictionary {
        out = apply_fuzzy_dictionary(&out, dictionary);
    }
    out = apply_dictionary(&out, dictionary);

    ProcessOutcome {
        text: out.trim().to_string(),
        stripped_fillers,
    }
}

// ---------------------------------------------------------------------------
// 0. Self-corrections — handle "X, I mean Y" and friends.
// ---------------------------------------------------------------------------

/// Patterns Murmr recognizes as the user correcting themselves mid-utterance.
/// Each one matches `<old>, <marker>, <new>` and rewrites it to `<new>`,
/// preserving the surrounding context so "the sky is blue, I mean red"
/// becomes "the sky is red".
///
/// Markers we know: "I mean" / "I meant" (with optional leading "or"),
/// "scratch that", "let me try that again", "actually I meant", "no wait".
fn apply_self_corrections(text: &str) -> String {
    // Pattern strategy: capture a single word being corrected (group 1)
    // followed by an optional comma, the marker, then 1–4 replacement words
    // (group 2). Replace the whole match with `<group2>`. The earlier
    // sentence prefix stays intact.
    //
    // We only replace ONE preceding word — multi-word corrections
    // ("the sky is bright blue, I mean red") still work because Whisper
    // tends to insert the marker right after the wrong word.
    let pats: &[&str] = &[
        // "X, or I mean Y" / "X, I mean Y" / "X or I mean Y"
        r"(?i)(\b\w+\b)\s*,?\s*(?:or\s+)?(?:i\s+(?:mean|meant)|actually\s+i\s+(?:mean|meant))\s+(\w+(?:\s+\w+){0,3})",
        // "X, scratch that, Y" / "X scratch that Y"
        r"(?i)(\b\w+\b)\s*,?\s*scratch\s+that\s*,?\s+(\w+(?:\s+\w+){0,4})",
        // "X, no wait, Y"
        r"(?i)(\b\w+\b)\s*,?\s*no\s+wait\s*,?\s+(\w+(?:\s+\w+){0,4})",
        // "X, let me try that again, Y"
        r"(?i)(\b\w+\b)\s*,?\s*let\s+me\s+try\s+that\s+again\s*,?\s+(\w+(?:\s+\w+){0,4})",
    ];

    let mut out = text.to_string();
    for pat in pats {
        let re = match Regex::new(pat) {
            Ok(r) => r,
            Err(_) => continue,
        };
        // Loop until the pattern stops matching — covers chained
        // corrections like "X, I mean Y, I mean Z" → "Z".
        loop {
            let replaced = re.replace(&out, "$2").to_string();
            if replaced == out {
                break;
            }
            out = replaced;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 1. Filler words
// ---------------------------------------------------------------------------

fn strip_fillers_with_counts(
    text: &str,
    fillers: &[String],
) -> (String, HashMap<String, i64>) {
    let mut counts: HashMap<String, i64> = HashMap::new();
    if fillers.is_empty() {
        return (text.to_string(), counts);
    }
    let alternation = fillers
        .iter()
        .filter(|f| !f.trim().is_empty())
        .map(|f| regex::escape(f.trim()))
        .collect::<Vec<_>>()
        .join("|");
    if alternation.is_empty() {
        return (text.to_string(), counts);
    }
    let pat = format!(r"(?i)\b(?:{alternation})\b[,]?");
    let re = match Regex::new(&pat) {
        Ok(r) => r,
        Err(_) => return (text.to_string(), counts),
    };
    // First pass: tally what we're about to strip.
    for m in re.find_iter(text) {
        let raw = m.as_str().trim_end_matches(',').to_lowercase();
        *counts.entry(raw).or_insert(0) += 1;
    }
    let stripped = re.replace_all(text, "").to_string();
    (collapse_whitespace(&stripped), counts)
}

// ---------------------------------------------------------------------------
// 2. Voice commands
// ---------------------------------------------------------------------------

struct VoiceCommand {
    /// Words that trigger this substitution. Matched whole-word, case-insensitive.
    triggers: &'static [&'static str],
    /// Text that replaces the trigger.
    replacement: &'static str,
}

const VOICE_COMMANDS: &[(&str, VoiceCommand)] = &[
    ("period", VoiceCommand {
        triggers: &["period"],
        replacement: ".",
    }),
    ("comma", VoiceCommand {
        triggers: &["comma"],
        replacement: ",",
    }),
    ("question", VoiceCommand {
        triggers: &["question mark"],
        replacement: "?",
    }),
    ("exclamation", VoiceCommand {
        triggers: &["exclamation point", "exclamation mark"],
        replacement: "!",
    }),
    ("new_line", VoiceCommand {
        triggers: &["new line"],
        replacement: "\n",
    }),
    ("new_paragraph", VoiceCommand {
        triggers: &["new paragraph"],
        replacement: "\n\n",
    }),
    // Code/prose symbols — all gated by the single `voice_command_symbols`
    // toggle (off by default, since these words appear in normal speech).
    ("sym_colon", VoiceCommand {
        triggers: &["colon"],
        replacement: ":",
    }),
    ("sym_semicolon", VoiceCommand {
        triggers: &["semicolon"],
        replacement: ";",
    }),
    ("sym_open_paren", VoiceCommand {
        triggers: &["open paren", "open parenthesis"],
        replacement: "(",
    }),
    ("sym_close_paren", VoiceCommand {
        triggers: &["close paren", "close parenthesis"],
        replacement: ")",
    }),
    ("sym_backtick", VoiceCommand {
        triggers: &["backtick", "back tick"],
        replacement: "`",
    }),
    ("sym_hyphen", VoiceCommand {
        triggers: &["hyphen"],
        replacement: "-",
    }),
];

fn apply_voice_commands(text: &str, settings: &Settings) -> String {
    let mut out = text.to_string();
    for (key, cmd) in VOICE_COMMANDS {
        let enabled = match *key {
            "period" => settings.voice_command_period,
            "comma" => settings.voice_command_comma,
            "question" => settings.voice_command_question,
            "exclamation" => settings.voice_command_exclamation,
            "new_line" => settings.voice_command_new_line,
            "new_paragraph" => settings.voice_command_new_paragraph,
            k if k.starts_with("sym_") => settings.voice_command_symbols,
            _ => false,
        };
        if !enabled {
            continue;
        }
        for trigger in cmd.triggers {
            // Whisper often inserts its own punctuation around trigger words
            // ("Hey there, comma." or "today, period."). To keep the result
            // clean we eat any whitespace + redundant punctuation on either
            // side of the trigger and replace the whole region with our
            // single canonical character.
            let pat = format!(
                r"(?i)\s*[.,;:!?]?\s*\b{}\b\s*[.,;:!?]?",
                regex::escape(trigger)
            );
            if let Ok(re) = Regex::new(&pat) {
                out = re.replace_all(&out, cmd.replacement).to_string();
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 3. Auto-capitalization
// ---------------------------------------------------------------------------

fn auto_capitalize(text: &str) -> String {
    if text.is_empty() {
        return text.to_string();
    }

    // First letter of the entire text uppercase.
    let mut chars: Vec<char> = text.chars().collect();
    capitalize_first_letter_at_or_after(&mut chars, 0);

    // After each sentence-ending punctuation, capitalize the next letter.
    for i in 0..chars.len() {
        if matches!(chars[i], '.' | '!' | '?') {
            capitalize_first_letter_at_or_after(&mut chars, i + 1);
        }
    }

    let mut out: String = chars.into_iter().collect();

    // Standalone "i" → "I" (with optional contractions: i'm, i've, i'll, i'd).
    let i_re = Regex::new(r"\bi(\b|')").unwrap();
    let mut result = String::with_capacity(out.len());
    let mut last = 0;
    for m in i_re.find_iter(&out) {
        result.push_str(&out[last..m.start()]);
        let span = &out[m.start()..m.end()];
        // Capitalize the first character only.
        let mut chars = span.chars();
        if let Some(c) = chars.next() {
            result.extend(c.to_uppercase());
            result.extend(chars);
        }
        last = m.end();
    }
    result.push_str(&out[last..]);
    out = result;

    out
}

fn capitalize_first_letter_at_or_after(chars: &mut Vec<char>, start: usize) {
    let mut i = start;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_alphabetic() && c.is_lowercase() {
            let upper: String = c.to_uppercase().collect();
            // Replace the single char at position `i`. (Most ASCII roundtrips
            // 1:1; Unicode lowercase→uppercase usually does too.)
            chars.splice(i..=i, upper.chars());
        }
        return;
    }
}

// ---------------------------------------------------------------------------
// 4. Auto-period
// ---------------------------------------------------------------------------

fn auto_period(text: &str) -> String {
    let trimmed = text.trim_end();
    if trimmed.is_empty() {
        return text.to_string();
    }
    let last = trimmed.chars().last().unwrap();
    if matches!(last, '.' | '!' | '?' | ',' | ';' | ':' | '\n') {
        return text.to_string();
    }
    format!("{}.", trimmed)
}

// ---------------------------------------------------------------------------
// 4b. Numbered lists
// ---------------------------------------------------------------------------
//
// Detects sequences like "One. ... Two. ... Three. ..." and reformats them
// into a real numbered list:
//
//   "Here are the colors. One. Blue. Two. Green. Three. Red."
//      ↓
//   "Here are the colors.\n1. Blue.\n2. Green.\n3. Red."
//
// Detection happens in two modes:
//   1. STRICT (no list intent detected): markers must form a strictly-
//      increasing 1,2,3,... sequence starting from 1, ≥2 items.
//   2. LOOSE (list intent detected in the surrounding text): markers must
//      form ANY monotonically-increasing sequence ≥2 items. Output is
//      always renumbered cleanly from 1.
//
// "Intent" = the user said something like "here are…", "the following…",
// "let me list…", "two things…", "three reasons…", etc. earlier in the
// transcript. When that's present we trust the speaker is actually
// enumerating, so we tolerate sloppier markers ("first… third…" or
// "two… three…").
//
// Recognized markers:
//   - Cardinal words: one, two, ..., twenty
//   - Ordinal words:  first, second, ..., twentieth
//   - Digits:         1, 2, ..., 20  (with optional ordinal suffix: 1st, 2nd)

const MARKER_WORDS: &[&str] = &[
    "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
    "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen",
    "eighteen", "nineteen", "twenty",
];

const ORDINAL_WORDS: &[&str] = &[
    "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth", "tenth",
    "eleventh", "twelfth", "thirteenth", "fourteenth", "fifteenth", "sixteenth", "seventeenth",
    "eighteenth", "nineteenth", "twentieth",
];

/// Map a marker token (lowercased, no period) to its numeric value, or None
/// if it isn't a recognized marker. Accepts cardinal words, ordinal words,
/// and digits with or without an ordinal suffix (1st / 2nd / 3rd / 4th).
fn marker_value(tok: &str) -> Option<u32> {
    let lower = tok.to_lowercase();

    // Numeric, with optional ordinal suffix. Strip the suffix and parse.
    let numeric_part = lower
        .trim_end_matches("st")
        .trim_end_matches("nd")
        .trim_end_matches("rd")
        .trim_end_matches("th");
    if let Ok(n) = numeric_part.parse::<u32>() {
        if (1..=20).contains(&n) {
            return Some(n);
        }
    }

    // Cardinal word.
    if let Some(i) = MARKER_WORDS.iter().position(|w| *w == lower) {
        return Some((i + 1) as u32);
    }

    // Ordinal word.
    if let Some(i) = ORDINAL_WORDS.iter().position(|w| *w == lower) {
        return Some((i + 1) as u32);
    }

    None
}

/// Heuristic: does the text contain phrases that signal the speaker is
/// about to enumerate items? When this returns true, we trust enumeration
/// intent enough to relax the marker-sequence rules (allow non-1 starts,
/// allow skipped numbers).
fn has_list_intent(text: &str) -> bool {
    // Catches: "here are…", "here is/'s…", "the following…", "let me list…",
    // "listing", "a few <thing>", "several <thing>", "couple of <thing>",
    // "<number> things|items|points|reasons|steps|ways|tips|notes|options|
    // choices|features|examples|categories|topics|questions|ideas".
    let pat = r"(?i)\b(?:here(?:'s| (?:are|is))|the following|let me list|listing|a few \w+|several \w+|couple of \w+|(?:one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|2|3|4|5|6|7|8|9|10|11|12)\s+(?:things|items|points|reasons|steps|ways|tips|notes|options|choices|features|examples|categories|topics|questions|ideas))\b";
    Regex::new(pat).map(|r| r.is_match(text)).unwrap_or(false)
}

fn apply_numbered_lists(text: &str) -> String {
    // Log a truncated preview so we can see what Whisper actually produced
    // (commas vs periods, digits vs words, etc.) when debugging "no markers
    // found" reports.
    let preview = if text.len() > 240 { format!("{}…", &text[..240]) } else { text.to_string() };
    crate::perf_log::append(&format!("[lists] input: {preview:?}"));

    let intent = has_list_intent(text);
    crate::perf_log::append(&format!("[lists] list intent detected: {intent}"));

    // Rust's `regex` crate doesn't support lookbehind, so we capture the
    // sentence-end character (or empty for start-of-text) explicitly in
    // group 1. Markers may be followed by period, comma, colon, `?`, or `!`.
    //
    // The optional connector clause (`and`/`or`/`for`/etc.) handles the
    // common pattern where the speaker joins items: "...item one, and two,
    // item two." That second "two" needs to match even though it's
    // separated from the previous comma by a connector word.
    //
    //   group 1 = leading `.!?,` or empty (start-of-text case)
    //   group 2 = marker token (cardinal / ordinal / digit)
    //   match   = "<g1>\s+(?:connector\s+)?<g2>[.,:!?]\s*"
    let re = match Regex::new(
        r"(?i)(^|[.!?,])\s+(?:(?:and|or|but|then|plus|next|finally|also|so|number|step|item|point|for|reason)\s+)?(\b(?:first|second|third|fourth|fifth|sixth|seventh|eighth|ninth|tenth|eleventh|twelfth|thirteenth|fourteenth|fifteenth|sixteenth|seventeenth|eighteenth|nineteenth|twentieth|one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|thirteen|fourteen|fifteen|sixteen|seventeen|eighteen|nineteen|twenty|\d{1,2}(?:st|nd|rd|th)?)\b)[.,:!?]\s*",
    ) {
        Ok(r) => r,
        Err(e) => {
            crate::perf_log::append(&format!("[lists] regex compile failed: {e}"));
            return text.to_string();
        }
    };

    // First pass: collect cut points + their numeric values. We cut RIGHT
    // AFTER the leading punctuation (group 1) so the previous sentence
    // keeps its terminal `.`/`,` and the connector word (`and`, `or`, …)
    // gets dropped along with the marker.
    let mut hits: Vec<(usize, usize, u32)> = Vec::new(); // (cut_start, end, value)
    for m in re.captures_iter(text) {
        let leading = m.get(1).unwrap();
        let token = m.get(2).unwrap().as_str();
        if let Some(value) = marker_value(token) {
            let whole_end = m.get(0).unwrap().end();
            // leading.end() is 0 for start-of-text matches and `pos+1` for
            // mid-text matches — exactly the cut point we want.
            hits.push((leading.end(), whole_end, value));
        }
    }
    crate::perf_log::append(&format!("[lists] found {} marker(s) in text", hits.len()));

    if hits.len() < 2 {
        return text.to_string();
    }

    // Threshold:
    //   - With intent: any monotonically-increasing chain ≥2 markers.
    //     "first… third…" or "two… three… five…" all count.
    //   - Without intent: strict 1,2,3,... starting from 1, ≥2 markers.
    //     Prevents corruption of prose like "...page one. Then on page
    //     three..." where the speaker isn't actually listing.
    let accepted: Vec<(usize, usize, u32)> = if intent {
        let mut accepted = Vec::new();
        let mut last_value = 0u32;
        for hit in &hits {
            if hit.2 > last_value {
                accepted.push(*hit);
                last_value = hit.2;
            }
        }
        accepted
    } else {
        let mut accepted = Vec::new();
        let mut expected = 1u32;
        for hit in &hits {
            if hit.2 == expected {
                accepted.push(*hit);
                expected += 1;
            } else {
                break;
            }
        }
        accepted
    };

    if accepted.len() < 2 {
        crate::perf_log::append(&format!(
            "[lists] only {} marker(s) form a valid sequence (intent={intent}) — leaving text alone",
            accepted.len()
        ));
        return text.to_string();
    }
    crate::perf_log::append(&format!(
        "[lists] reformatting {} items as numbered list (intent={intent})",
        accepted.len()
    ));

    // Rebuild the string, replacing each accepted marker with its sequential
    // canonical form (1, 2, 3, ...) and prepending a newline if not already
    // at start. We renumber from 1 unconditionally so a "first… third…"
    // pattern outputs "1. ... 2. ..." cleanly — the user wanted a list, the
    // specific numbers they spoke were just enumeration cues.
    let mut out = String::with_capacity(text.len() + accepted.len() * 2);
    let mut cursor = 0;
    for (i, (start, end, _value)) in accepted.iter().enumerate() {
        out.push_str(text[cursor..*start].trim_end());
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!("{}. ", i + 1));
        cursor = *end;
    }
    // Trailing text after the last marker.
    out.push_str(&text[cursor..]);
    out
}

// ---------------------------------------------------------------------------
// 4c. Bulleted lists
// ---------------------------------------------------------------------------
//
// Mirrors numbered lists for unordered enumeration. The speaker says
// "bullet" (or "bullet point") before each item:
//
//   "Groceries. Bullet milk. Bullet eggs. Bullet bread."
//      ↓
//   "Groceries.\n- Milk.\n- Eggs.\n- Bread."
//
// Requires ≥2 markers so a stray "bullet" in prose isn't reformatted. We
// deliberately do NOT use "dash" as a marker — it collides with the hyphen
// symbol command and with ordinary usage.

fn apply_bulleted_lists(text: &str) -> String {
    let re = match Regex::new(
        r"(?i)(^|[.!?,])\s+(?:(?:and|or|then|also|next|plus)\s+)?bullet(?:\s+point)?\s*[.,:!?]?\s*",
    ) {
        Ok(r) => r,
        Err(_) => return text.to_string(),
    };

    let mut hits: Vec<(usize, usize)> = Vec::new(); // (cut_start, end)
    for m in re.captures_iter(text) {
        let leading = m.get(1).unwrap();
        let whole_end = m.get(0).unwrap().end();
        hits.push((leading.end(), whole_end));
    }
    if hits.len() < 2 {
        return text.to_string();
    }

    let mut out = String::with_capacity(text.len() + hits.len() * 2);
    let mut cursor = 0;
    for (start, end) in &hits {
        out.push_str(text[cursor..*start].trim_end());
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("- ");
        cursor = *end;
    }
    out.push_str(&text[cursor..]);
    out
}

// ---------------------------------------------------------------------------
// 4d-bis. Natural (unmarked) lists
// ---------------------------------------------------------------------------
//
// Detects an enumerative lead-in immediately followed by a comma/"and" series
// of ≥3 short items and reformats it as a clean list — WITHOUT the speaker
// having to say "one/two" or "bullet":
//
//   "I need to buy milk, eggs, bread, and cheese"
//      ↓
//   "I need to buy:
//    - Milk
//    - Eggs
//    - Bread
//    - Cheese"
//
// Conservative on purpose: it needs a recognizable lead-in cue, ≥3 short
// parallel items (no sentence punctuation or subordinating conjunctions
// inside an item), and the series must close the utterance — so ordinary
// prose that merely contains commas is left untouched.

/// Cues that imply an ORDERED list (→ numbered output).
const ORDERED_CUES: &[&str] = &[
    "steps are", "steps", "instructions are", "instructions", "process is",
    "directions are", "directions", "in order",
];
/// Cues that imply an UNORDERED list (→ bulleted output). Each sits directly
/// before the first item.
const UNORDERED_CUES: &[&str] = &[
    "need to", "want to", "have to", "got to", "remember to", "make sure to",
    "don't forget to", "buy", "get", "grab", "pick up", "purchase", "add",
    "including", "include", "the following", "namely", "such as",
    "send me", "send", "give me", "show me", "bring", "email me", "attach",
    "provide", "cover", "discuss",
    "are", "is", "am", "were", "was",
];

fn apply_natural_lists(text: &str) -> String {
    // Already turned into a list upstream (explicit markers) — leave it.
    if text.contains("\n- ") || text.contains("\n1. ") {
        return text.to_string();
    }

    let mut cues: Vec<(&str, bool)> = Vec::new(); // (cue, ordered)
    for c in ORDERED_CUES {
        cues.push((c, true));
    }
    for c in UNORDERED_CUES {
        cues.push((c, false));
    }
    // Longest cue first so multi-word cues win in the alternation.
    cues.sort_by_key(|(c, _)| std::cmp::Reverse(c.len()));
    let alternation = cues
        .iter()
        .map(|(c, _)| regex::escape(c))
        .collect::<Vec<_>>()
        .join("|");

    // Greedy prefix so we anchor on the LAST cue before the series.
    let pat =
        format!(r"(?is)^(?P<prefix>.+\b(?P<cue>{alternation}))\s*:?\s+(?P<series>.+?)[.!?]?\s*$");
    let re = match Regex::new(&pat) {
        Ok(r) => r,
        Err(_) => return text.to_string(),
    };

    let trimmed = text.trim();
    let caps = match re.captures(trimmed) {
        Some(c) => c,
        None => return text.to_string(),
    };
    let prefix = caps.name("prefix").unwrap().as_str().trim();
    let cue = caps.name("cue").unwrap().as_str().to_lowercase();
    let series = caps.name("series").unwrap().as_str().trim();

    let items = split_series(series);
    if items.len() < 3 {
        return text.to_string();
    }

    // Every item must be a short, self-contained phrase — reject if any looks
    // like a full clause (too long, embedded sentence punctuation, or a
    // subordinating/coordinating conjunction that signals prose, not a list).
    const BANNED_SUBSTR: &[&str] = &[
        " because ", " although ", " however ", " therefore ", " so that ",
        " which ", " that ", " but ", " nor ", " while ", " since ",
    ];
    for it in &items {
        let wc = it.split_whitespace().count();
        if wc == 0 || wc > 6 {
            return text.to_string();
        }
        if it.contains(['.', '!', '?', ';', ':']) {
            return text.to_string();
        }
        let padded = format!(" {} ", it.to_lowercase());
        if BANNED_SUBSTR.iter().any(|b| padded.contains(b)) {
            return text.to_string();
        }
    }

    let ordered = ORDERED_CUES.iter().any(|c| c.eq_ignore_ascii_case(&cue));

    let mut out = String::with_capacity(trimmed.len() + items.len() * 3);
    out.push_str(prefix);
    if !prefix.ends_with(':') {
        out.push(':');
    }
    for (i, it) in items.iter().enumerate() {
        out.push('\n');
        if ordered {
            out.push_str(&format!("{}. ", i + 1));
        } else {
            out.push_str("- ");
        }
        out.push_str(&capitalize_first(it));
    }
    out
}

/// Split a comma/"and"/"or" series into its items, handling both the
/// oxford-comma form ("a, b, and c") and the non-oxford form ("a, b and c").
fn split_series(series: &str) -> Vec<String> {
    let mut parts: Vec<String> = series
        .split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();

    // The final comma-part may itself join the last two items with "and"/"or".
    if let Some(last) = parts.pop() {
        let low = last.to_lowercase();
        let cut = low
            .rfind(" and ")
            .map(|i| (i, 5usize))
            .or_else(|| low.rfind(" or ").map(|i| (i, 4usize)));
        match cut {
            Some((i, len)) => {
                let a = last[..i].trim().to_string();
                let b = last[i + len..].trim().to_string();
                if !a.is_empty() {
                    parts.push(a);
                }
                if !b.is_empty() {
                    parts.push(b);
                }
            }
            None => parts.push(last),
        }
    }

    // Strip a leading "and "/"or " left on any item (e.g. from ", and cheese").
    parts
        .into_iter()
        .map(|p| {
            let low = p.to_lowercase();
            if low.starts_with("and ") {
                p[4..].trim().to_string()
            } else if low.starts_with("or ") {
                p[3..].trim().to_string()
            } else {
                p
            }
        })
        .filter(|p| !p.is_empty())
        .collect()
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// 4d. Fuzzy proper-noun correction
// ---------------------------------------------------------------------------
//
// Whisper's initial-prompt bias helps with names but isn't reliable. This
// stage takes the enabled single-word "word" dictionary entries and snaps
// near-miss tokens in the transcript to the intended spelling — conservative
// by design: the token and target must share a first letter, be close in
// length, and be within a small edit distance, so unrelated words are never
// touched.

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr: Vec<usize> = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1].eq_ignore_ascii_case(&b[j - 1]) { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

fn best_fuzzy_match<'a>(tok: &str, words: &[&'a str]) -> Option<&'a str> {
    // Exact match (case-insensitive) → already correct, leave it.
    if words.iter().any(|w| w.eq_ignore_ascii_case(tok)) {
        return None;
    }
    let tok_lower = tok.to_lowercase();
    let tok_first = tok.chars().next();
    let tlen = tok.chars().count();

    let mut best: Option<(&'a str, usize)> = None;
    for w in words {
        // First letter must match — names/brands almost always get the
        // initial right, and this rules out most coincidental matches.
        let w_first = w.chars().next();
        match (tok_first, w_first) {
            (Some(t), Some(c)) if t.eq_ignore_ascii_case(&c) => {}
            _ => continue,
        }
        let wlen = w.chars().count();
        if (wlen as i32 - tlen as i32).abs() > 2 {
            continue;
        }
        let d = levenshtein(&tok_lower, &w.to_lowercase());
        // Distance ceiling scales gently with length.
        let max_d = if tlen <= 5 { 1 } else { 2 };
        if d >= 1 && d <= max_d && best.map_or(true, |(_, bd)| d < bd) {
            best = Some((w, d));
        }
    }
    best.map(|(w, _)| w)
}

fn apply_fuzzy_dictionary(text: &str, entries: &[DictionaryEntry]) -> String {
    let words: Vec<&str> = entries
        .iter()
        .filter(|e| e.enabled && e.entry_type == "word")
        // Only single alphabetic tokens ≥5 chars are safe to fuzzy-match —
        // short words collide with common vocabulary too easily.
        .map(|e| e.trigger.as_str())
        .filter(|w| w.chars().count() >= 5 && w.chars().all(|c| c.is_alphabetic()))
        .collect();
    if words.is_empty() {
        return text.to_string();
    }

    let re = match Regex::new(r"[A-Za-z]{5,}") {
        Ok(r) => r,
        Err(_) => return text.to_string(),
    };
    let mut result = String::with_capacity(text.len());
    let mut last = 0;
    for m in re.find_iter(text) {
        result.push_str(&text[last..m.start()]);
        let tok = m.as_str();
        match best_fuzzy_match(tok, &words) {
            Some(fixed) => result.push_str(fixed),
            None => result.push_str(tok),
        }
        last = m.end();
    }
    result.push_str(&text[last..]);
    result
}

// ---------------------------------------------------------------------------
// 5 + 6. Dictionary replacements + snippet expansion
// ---------------------------------------------------------------------------

fn apply_dictionary(text: &str, entries: &[DictionaryEntry]) -> String {
    let mut out = text.to_string();
    for e in entries {
        if !e.enabled {
            continue;
        }
        if e.entry_type == "word" {
            // Words are passed to Whisper as initial_prompt — no
            // post-processing replacement to do.
            continue;
        }
        let expansion = match e.expansion.as_deref() {
            Some(s) if !s.is_empty() => s,
            _ => continue,
        };
        let pat = if e.is_regex {
            e.trigger.clone()
        } else {
            // Whole-word, case-insensitive literal match.
            format!(r"(?i)\b{}\b", regex::escape(&e.trigger))
        };
        if let Ok(re) = Regex::new(&pat) {
            out = re.replace_all(&out, expansion).to_string();
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for c in s.chars() {
        if c == ' ' || c == '\t' {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    // Tidy up space before punctuation that filler removal may have left.
    let cleaned = out
        .replace(" ,", ",")
        .replace(" .", ".")
        .replace(" !", "!")
        .replace(" ?", "?")
        .replace(" ;", ";")
        .replace(" :", ":");
    cleaned.trim().to_string()
}

/// Build a Whisper `initial_prompt` from the dictionary's Word entries.
/// Whisper biases its decoding toward tokens it sees in this prompt, which
/// helps with proper nouns (e.g. "Murmr", "whisper.cpp") that the model
/// would otherwise mis-spell.
pub fn build_initial_prompt(dictionary: &[DictionaryEntry]) -> Option<String> {
    let words: Vec<&str> = dictionary
        .iter()
        .filter(|e| e.enabled && e.entry_type == "word")
        .map(|e| e.trigger.as_str())
        .collect();
    if words.is_empty() {
        return None;
    }
    // Cap length — Whisper's prompt context is ~200 tokens (plan §13 #6).
    // 1 word ≈ 1.3 tokens average; so ~150 words is the safe ceiling.
    let mut joined = String::new();
    for (i, w) in words.iter().enumerate() {
        if i > 0 {
            joined.push_str(", ");
        }
        joined.push_str(w);
        if joined.len() > 600 {
            break;
        }
    }
    Some(joined)
}
