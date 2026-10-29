//! Learn dictionary suggestions from the user's own corrections.
//!
//! When the user fixes a transcript (edit-last bubble, or a past entry in
//! History) we diff the original against the edited text word by word. A
//! changed word that looks like a name, brand or piece of jargon becomes a
//! *suggestion*: a `word` entry (biases Whisper toward it) and, when safe, a
//! `replacement` entry (misheard → correct). Ordinary grammar, punctuation
//! and plain-English word swaps are ignored.
//!
//! This module is pure (no I/O) so it is cheap to call and easy to test. The
//! DB layer stores, dedupes and counts what it returns.

use serde::Serialize;

/// One dictionary entry we think the user would want.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Candidate {
    /// `"word"` or `"replacement"` — same values as `dictionary_entries.type`.
    pub kind: &'static str,
    pub trigger: String,
    /// The replacement text. `None` for `word` candidates.
    pub expansion: Option<String>,
}

/// Longest phrase (in words) we accept on either side of a single change.
/// Bigger hunks are rewrites, not misheard words.
const MAX_REMOVED_WORDS: usize = 4;
const MAX_ADDED_WORDS: usize = 3;
/// Guard against pathological inputs: skip the diff above this many cells.
const MAX_DIFF_CELLS: usize = 1_000_000;
/// Upper bound on suggestions from one correction.
const MAX_CANDIDATES: usize = 6;

#[derive(Debug)]
struct Token {
    /// Word with edge punctuation removed, original casing.
    core: String,
    lower: String,
    /// First word of the text or of a sentence.
    sentence_start: bool,
}

fn trim_edge_punct(s: &str) -> &str {
    s.trim_matches(|c: char| {
        matches!(
            c,
            '.' | ',' | ';' | ':' | '!' | '?' | '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}'
                | '…' | '“' | '”' | '‘' | '’' | '«' | '»' | '-' | '–' | '—' | '*' | '_'
        )
    })
}

fn tokenize(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut next_is_start = true;
    for raw in text.split_whitespace() {
        let core = trim_edge_punct(raw);
        let ends_sentence = raw
            .trim_end_matches(|c: char| matches!(c, '"' | '\'' | ')' | '”' | '’'))
            .ends_with(['.', '!', '?']);
        if !core.is_empty() {
            out.push(Token {
                core: core.to_string(),
                lower: core.to_lowercase(),
                sentence_start: next_is_start,
            });
            next_is_start = false;
        }
        if ends_sentence {
            next_is_start = true;
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Equal(usize, usize),
    Del(usize),
    Ins(usize),
}

/// Word-level LCS alignment on lowercase words. Returns `None` when the
/// inputs are too large to diff cheaply.
fn align(a: &[Token], b: &[Token]) -> Option<Vec<Op>> {
    // Strip the common prefix/suffix first; most corrections touch one spot.
    let mut pre = 0;
    while pre < a.len() && pre < b.len() && a[pre].lower == b[pre].lower {
        pre += 1;
    }
    let mut suf = 0;
    while suf < a.len() - pre
        && suf < b.len() - pre
        && a[a.len() - 1 - suf].lower == b[b.len() - 1 - suf].lower
    {
        suf += 1;
    }
    let am = &a[pre..a.len() - suf];
    let bm = &b[pre..b.len() - suf];
    let (n, m) = (am.len(), bm.len());
    if (n + 1).saturating_mul(m + 1) > MAX_DIFF_CELLS {
        return None;
    }

    // dp[i][j] = LCS length of am[i..] and bm[j..]
    let w = m + 1;
    let mut dp = vec![0u32; (n + 1) * w];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i * w + j] = if am[i].lower == bm[j].lower {
                dp[(i + 1) * w + j + 1] + 1
            } else {
                dp[(i + 1) * w + j].max(dp[i * w + j + 1])
            };
        }
    }

    let mut ops = Vec::with_capacity(pre + n + m + suf);
    for k in 0..pre {
        ops.push(Op::Equal(k, k));
    }
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && am[i].lower == bm[j].lower {
            ops.push(Op::Equal(pre + i, pre + j));
            i += 1;
            j += 1;
        } else if j < m && (i == n || dp[i * w + j + 1] >= dp[(i + 1) * w + j]) {
            ops.push(Op::Ins(pre + j));
            j += 1;
        } else {
            ops.push(Op::Del(pre + i));
            i += 1;
        }
    }
    for k in 0..suf {
        ops.push(Op::Equal(a.len() - suf + k, b.len() - suf + k));
    }
    Some(ops)
}

/// Does this (corrected) word look like a name, brand or jargon rather than
/// an ordinary English word?
fn is_name_like(t: &Token) -> bool {
    let chars: Vec<char> = t.core.chars().collect();
    if chars.len() < 2 || !chars.iter().any(|c| c.is_alphabetic()) {
        return false;
    }
    if COMMON_WORDS.binary_search(&t.lower.as_str()).is_ok() {
        return false;
    }
    let has_digit = chars.iter().any(|c| c.is_ascii_digit());
    // Letters mixed with digits: GPT4, M2, v0.1, 3D.
    if has_digit {
        return true;
    }
    // Tech punctuation inside the word: Node.js, C++, C#, .NET.
    if t.core.contains(['.', '+', '#', '/', '@']) {
        return true;
    }
    // Uppercase after the first letter: GitHub, iPhone, API, McDonald.
    if chars.iter().skip(1).any(|c| c.is_uppercase()) {
        return true;
    }
    // Starts lowercase but not a common word: too ambiguous (could be a
    // typo fix or plain vocabulary we don't list).
    if !chars[0].is_uppercase() {
        return false;
    }
    // Capitalised mid-sentence → proper noun. At a sentence start the
    // capital is just grammar, so require a longer, uncommon word.
    !t.sentence_start || chars.len() >= 4
}

fn is_common(lower: &str) -> bool {
    COMMON_WORDS.binary_search(&lower).is_ok()
}

fn join_core(tokens: &[&Token]) -> String {
    tokens
        .iter()
        .map(|t| t.core.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A replacement trigger is matched as `\b<trigger>\b`, so it must start and
/// end with a word character to ever match.
fn trigger_is_matchable(s: &str) -> bool {
    let first = s.chars().next();
    let last = s.chars().last();
    matches!(first, Some(c) if c.is_alphanumeric()) && matches!(last, Some(c) if c.is_alphanumeric())
}

/// Diff `original` against `edited` and return dictionary candidates.
pub fn suggest(original: &str, edited: &str) -> Vec<Candidate> {
    let a = tokenize(original);
    let b = tokenize(edited);
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let Some(ops) = align(&a, &b) else {
        return Vec::new();
    };

    // Heavy rewrite? Then the user changed what they meant, not what
    // Whisper heard — learn nothing.
    let changed = ops.iter().filter(|o| !matches!(o, Op::Equal(..))).count();
    if a.len() >= 6 && changed * 2 > a.len() + b.len() {
        return Vec::new();
    }

    let mut out: Vec<Candidate> = Vec::new();
    let mut push = |c: Candidate, out: &mut Vec<Candidate>| {
        let dup = out.iter().any(|o| {
            o.kind == c.kind
                && o.trigger.eq_ignore_ascii_case(&c.trigger)
                && o.expansion.as_deref().map(str::to_lowercase)
                    == c.expansion.as_deref().map(str::to_lowercase)
        });
        if !dup && out.len() < MAX_CANDIDATES {
            out.push(c);
        }
    };

    let mut k = 0;
    while k < ops.len() {
        match ops[k] {
            Op::Equal(i, j) => {
                // Same word, different casing: "github" → "GitHub".
                if a[i].core != b[j].core && is_name_like(&b[j]) {
                    push(
                        Candidate {
                            kind: "word",
                            trigger: b[j].core.clone(),
                            expansion: None,
                        },
                        &mut out,
                    );
                }
                k += 1;
            }
            _ => {
                // Collect one hunk of consecutive non-equal ops.
                let mut removed: Vec<&Token> = Vec::new();
                let mut added: Vec<&Token> = Vec::new();
                while k < ops.len() {
                    match ops[k] {
                        Op::Del(i) => removed.push(&a[i]),
                        Op::Ins(j) => added.push(&b[j]),
                        Op::Equal(..) => break,
                    }
                    k += 1;
                }
                hunk_candidates(&removed, &added, &mut out, &mut push);
            }
        }
    }
    out
}

fn hunk_candidates(
    removed: &[&Token],
    added: &[&Token],
    out: &mut Vec<Candidate>,
    push: &mut impl FnMut(Candidate, &mut Vec<Candidate>),
) {
    // Only substitutions teach us anything about what Whisper misheard.
    if removed.is_empty() || added.is_empty() {
        return;
    }
    if removed.len() > MAX_REMOVED_WORDS || added.len() > MAX_ADDED_WORDS {
        return;
    }
    if !added.iter().any(|t| is_name_like(t)) {
        return;
    }
    let added_text = join_core(added);
    let removed_text = join_core(removed);
    if added_text.chars().count() > 40 {
        return;
    }

    push(
        Candidate {
            kind: "word",
            trigger: added_text.clone(),
            expansion: None,
        },
        out,
    );

    // A replacement rewrites every future match, case-insensitively. Never
    // learn one whose trigger is a single everyday word ("cloud" → "Claude"
    // would break every real "cloud"); the `word` entry handles those.
    let single_common = removed.len() == 1 && is_common(&removed[0].lower);
    if !single_common
        && removed_text.to_lowercase() != added_text.to_lowercase()
        && removed_text.chars().count() >= 2
        && trigger_is_matchable(&removed_text)
    {
        push(
            Candidate {
                kind: "replacement",
                trigger: removed_text,
                expansion: Some(added_text),
            },
            out,
        );
    }
}

/// ~450 frequent English words, sorted for binary search. A corrected word
/// in this list is treated as ordinary vocabulary, never as a name.
static COMMON_WORDS: &[&str] = &[
    "a", "able", "about", "above", "accept", "across", "act", "actually", "add", "after",
    "again", "against", "ago", "agree", "ah", "air", "all", "almost", "alone", "along",
    "already", "also", "although", "always", "am", "among", "an", "and", "another", "answer",
    "any", "anyone", "anything", "anyway", "are", "area", "around", "as", "ask", "at",
    "away", "back", "bad", "be", "because", "become", "been", "before", "began", "begin",
    "behind", "being", "believe", "below", "best", "better", "between", "big", "bit", "black",
    "body", "book", "both", "bring", "build", "business", "but", "buy", "by", "call",
    "came", "can", "can't", "car", "care", "case", "cause", "change", "check", "child",
    "city", "class", "clear", "close", "cloud", "code", "cold", "come", "company", "could",
    "couldn't", "course", "cut", "data", "day", "days", "did", "didn't", "different", "do",
    "does", "doesn't", "doing", "don't", "done", "door", "down", "during", "each", "early",
    "easy", "eat", "either", "else", "end", "enough", "even", "evening", "ever", "every",
    "everyone", "everything", "eye", "face", "fact", "fall", "family", "far", "feel", "few",
    "file", "find", "fine", "first", "fix", "for", "form", "found", "free", "friday",
    "friend", "from", "front", "full", "game", "gave", "get", "gets", "getting", "give",
    "go", "goes", "going", "gone", "good", "got", "great", "group", "grow", "had",
    "half", "hand", "happen", "hard", "has", "have", "haven't", "having", "he", "head",
    "hear", "heard", "help", "her", "here", "high", "him", "his", "hold", "home",
    "hope", "hour", "house", "how", "however", "i", "i'd", "i'll", "i'm", "i've",
    "idea", "if", "important", "in", "into", "is", "isn't", "issue", "it", "it's",
    "its", "job", "just", "keep", "kind", "knew", "know", "large", "last", "late",
    "later", "lead", "learn", "least", "leave", "left", "less", "let", "let's", "life",
    "light", "like", "line", "list", "little", "live", "long", "look", "lot", "love",
    "made", "make", "man", "many", "may", "maybe", "me", "mean", "meet", "meeting",
    "might", "mind", "minute", "monday", "money", "month", "more", "morning", "most", "move",
    "much", "must", "my", "name", "need", "never", "new", "next", "nice", "night",
    "no", "not", "note", "nothing", "now", "number", "of", "off", "office", "often",
    "oh", "ok", "okay", "old", "on", "once", "one", "only", "open", "or",
    "order", "other", "our", "out", "over", "own", "page", "part", "past", "pay",
    "people", "per", "person", "pick", "place", "plan", "play", "please", "point", "possible",
    "post", "power", "pretty", "probably", "problem", "project", "put", "question", "quick", "quite",
    "rather", "read", "ready", "real", "really", "reason", "remember", "report", "rest", "right",
    "room", "run", "said", "same", "saturday", "saw", "say", "school", "second", "see",
    "seem", "seen", "send", "sent", "set", "she", "should", "shouldn't", "show", "side",
    "since", "small", "so", "some", "someone", "something", "sometimes", "soon", "sorry", "sound",
    "speak", "start", "state", "still", "stop", "story", "such", "sunday", "sure", "system",
    "take", "talk", "team", "tell", "than", "thank", "thanks", "that", "that's", "the",
    "their", "them", "then", "there", "there's", "these", "they", "they're", "thing", "things",
    "think", "this", "those", "though", "thought", "three", "through", "thursday", "time", "to",
    "today", "together", "told", "tomorrow", "too", "took", "top", "try", "tuesday", "turn",
    "two", "under", "until", "up", "update", "us", "use", "used", "using", "very",
    "wait", "want", "was", "wasn't", "watch", "water", "way", "we", "we'll", "we're",
    "wednesday", "week", "well", "went", "were", "what", "what's", "when", "where", "whether",
    "which", "while", "white", "who", "whole", "why", "will", "with", "within", "without",
    "won't", "word", "work", "world", "would", "wouldn't", "write", "wrong", "yeah", "year",
    "yes", "yesterday", "yet", "you", "you'll", "you're", "your",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(c: &[Candidate]) -> Vec<(String, String, Option<String>)> {
        c.iter()
            .map(|c| (c.kind.to_string(), c.trigger.clone(), c.expansion.clone()))
            .collect()
    }

    #[test]
    fn common_words_sorted() {
        let mut sorted = COMMON_WORDS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, COMMON_WORDS, "COMMON_WORDS must be sorted and unique");
    }

    #[test]
    fn misheard_name_gives_word_and_replacement() {
        let c = suggest(
            "We deploy it with cooper netties every week.",
            "We deploy it with Kubernetes every week.",
        );
        assert_eq!(
            kinds(&c),
            vec![
                ("word".into(), "Kubernetes".into(), None),
                ("replacement".into(), "cooper netties".into(), Some("Kubernetes".into())),
            ]
        );
    }

    #[test]
    fn single_common_word_gives_word_only() {
        let c = suggest("I asked cloud about it.", "I asked Claude about it.");
        assert_eq!(kinds(&c), vec![("word".into(), "Claude".into(), None)]);
    }

    #[test]
    fn casing_fix_gives_word() {
        let c = suggest("push it to github tonight", "push it to GitHub tonight");
        assert_eq!(kinds(&c), vec![("word".into(), "GitHub".into(), None)]);
    }

    #[test]
    fn grammar_and_punctuation_ignored() {
        assert!(suggest("their going to the store", "they're going to the store").is_empty());
        assert!(suggest("send it to me today", "Send it to me today.").is_empty());
        assert!(suggest("I has a idea", "I have an idea").is_empty());
        assert!(suggest("the sink is broken", "the sync is broken").is_empty());
    }

    #[test]
    fn sentence_start_capital_needs_uncommon_word() {
        // "The" at a sentence start is grammar, not a name.
        assert!(suggest("the plan works", "The plan works").is_empty());
        let c = suggest("Wisper is great", "Whisper is great");
        assert_eq!(kinds(&c)[0], ("word".into(), "Whisper".into(), None));
    }

    #[test]
    fn jargon_with_digits_and_dots() {
        let c = suggest("we use next js and gpt four", "we use Next.js and GPT-4");
        let words: Vec<String> = c
            .iter()
            .filter(|c| c.kind == "word")
            .map(|c| c.trigger.clone())
            .collect();
        assert!(words.contains(&"Next.js".to_string()), "{c:?}");
        assert!(words.contains(&"GPT-4".to_string()), "{c:?}");
    }

    #[test]
    fn rewrite_is_ignored() {
        assert!(suggest(
            "Can you send me the report by Friday afternoon please",
            "Let's talk about Murmr pricing on Monday with Dana instead",
        )
        .is_empty());
    }

    #[test]
    fn unchanged_or_empty_gives_nothing() {
        assert!(suggest("Hello there", "Hello there").is_empty());
        assert!(suggest("", "Kubernetes").is_empty());
        assert!(suggest("Kubernetes", "").is_empty());
    }

    #[test]
    fn short_text_full_substitution() {
        let c = suggest("murmur", "Murmr");
        assert_eq!(
            kinds(&c),
            vec![
                ("word".into(), "Murmr".into(), None),
                ("replacement".into(), "murmur".into(), Some("Murmr".into())),
            ]
        );
    }

    #[test]
    fn large_input_does_not_panic() {
        let a = "word ".repeat(3000);
        let b = "other ".repeat(3000);
        let _ = suggest(&a, &b);
    }
}
