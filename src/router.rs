//! Rule-based routing of free-form input to the most relevant session.

/// Split "@target rest" into (target, rest). Only a leading mention counts.
pub fn split_mention(input: &str) -> (Option<&str>, &str) {
    let t = input.trim_start();
    if let Some(rest) = t.strip_prefix('@') {
        let (name, body) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        if !name.is_empty() {
            return (Some(name), body.trim_start());
        }
    }
    (None, input)
}

/// Word tokens for ASCII text, character bigrams for everything else
/// (so Japanese works without whitespace).
pub fn tokens(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut out = vec![];
    let mut word = String::new();
    let mut wide = String::new();
    let flush_word = |w: &mut String, out: &mut Vec<String>| {
        if w.chars().count() >= 3 {
            out.push(std::mem::take(w));
        } else {
            w.clear();
        }
    };
    let flush_wide = |w: &mut String, out: &mut Vec<String>| {
        let cs: Vec<char> = w.chars().collect();
        for pair in cs.windows(2) {
            out.push(pair.iter().collect());
        }
        w.clear();
    };
    for c in lower.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            flush_wide(&mut wide, &mut out);
            word.push(c);
        } else if c.is_alphanumeric() {
            flush_word(&mut word, &mut out);
            wide.push(c);
        } else {
            flush_word(&mut word, &mut out);
            flush_wide(&mut wide, &mut out);
        }
    }
    flush_word(&mut word, &mut out);
    flush_wide(&mut wide, &mut out);
    out
}

/// Pick the session whose text overlaps most with `input`.
/// Returns None when nothing matches or the best score is tied.
pub fn best_match(input: &str, haystacks: &[String]) -> Option<usize> {
    let toks = tokens(input);
    let scores: Vec<usize> = haystacks
        .iter()
        .map(|h| {
            let h = h.to_lowercase();
            toks.iter().filter(|t| h.contains(t.as_str())).count()
        })
        .collect();
    let max = *scores.iter().max()?;
    if max == 0 {
        return None;
    }
    let mut best = scores.iter().enumerate().filter(|(_, s)| **s == max);
    let first = best.next()?.0;
    if best.next().is_some() {
        None
    } else {
        Some(first)
    }
}

/// Resolve "@3" or "@name" (prefix match) to a session index.
pub fn resolve_mention(m: &str, names: &[String]) -> Option<usize> {
    if let Ok(n) = m.parse::<usize>() {
        return (1..=names.len()).contains(&n).then(|| n - 1);
    }
    let m = m.to_lowercase();
    names.iter().position(|n| n.to_lowercase().starts_with(&m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mention_split() {
        assert_eq!(split_mention("@2 fix bug"), (Some("2"), "fix bug"));
        assert_eq!(split_mention("fix @2"), (None, "fix @2"));
    }

    #[test]
    fn mention_resolve() {
        let names = vec!["claude#1".to_string(), "api".to_string()];
        assert_eq!(resolve_mention("2", &names), Some(1));
        assert_eq!(resolve_mention("ap", &names), Some(1));
        assert_eq!(resolve_mention("9", &names), None);
    }

    #[test]
    fn routes_by_overlap() {
        let h = vec![
            "working on the parser module in rust".to_string(),
            "notion task for baby shopping".to_string(),
        ];
        assert_eq!(best_match("fix the parser", &h), Some(0));
        assert_eq!(best_match("xyz", &h), None);
    }

    #[test]
    fn routes_japanese() {
        let h = vec!["家の整理タスク".to_string(), "rust parser".to_string()];
        assert_eq!(best_match("整理のタスクを追加", &h), Some(0));
    }

    #[test]
    fn tie_is_ambiguous() {
        let h = vec!["parser".to_string(), "parser".to_string()];
        assert_eq!(best_match("parser", &h), None);
    }
}
