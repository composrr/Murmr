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
    let mut out = text.to_string();

    // Self-corrections fire FIRST so the rest of the pipeline operates on
    // the already-fixed sentence. Always-on for now (no setting yet).
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
