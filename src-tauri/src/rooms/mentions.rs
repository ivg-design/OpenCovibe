//! Explicit human @mentions supplement the recipient picker.
use super::models::Participant;

pub fn recipients(body: &str, participants: &[Participant]) -> Option<Vec<String>> {
    let mut candidates: Vec<_> = participants.iter().collect();
    candidates.sort_by_key(|p| std::cmp::Reverse(p.name.len()));
    let mut ids = Vec::new();
    let mut fence = false;
    let mut inline = false;
    for line in body.lines() {
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            fence = !fence;
            continue;
        }
        if fence {
            continue;
        }
        for (pos, c) in line.char_indices() {
            if c == '`' {
                inline = !inline;
            }
            if inline || c != '@' {
                continue;
            }
            let previous = line[..pos].chars().next_back();
            if previous.is_some_and(|p| {
                !p.is_whitespace() && !matches!(p, '(' | '[' | '{' | '"' | '\'' | ',')
            }) {
                continue;
            }
            let rest = &line[pos + 1..];
            let matches = |name: &str| {
                let prefix: String = rest.chars().take(name.chars().count()).collect();
                prefix.to_lowercase() == name.to_lowercase()
                    && rest
                        .get(prefix.len()..)
                        .and_then(|s| s.chars().next())
                        .is_none_or(|c| !c.is_alphanumeric() && !matches!(c, '_' | '-'))
            };
            if matches("everyone") {
                return Some(vec![]);
            }
            if let Some(p) = candidates.iter().find(|p| matches(&p.name)) {
                if !ids.contains(&p.id) {
                    ids.push(p.id.clone());
                }
            }
        }
        inline = false;
    }
    if ids.is_empty() {
        None
    } else {
        Some(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn peers() -> Vec<Participant> {
        ["Lead", "Claude delegate", "Codex delegate"]
            .into_iter()
            .enumerate()
            .map(|(i, n)| Participant {
                id: i.to_string(),
                name: n.into(),
                ..Default::default()
            })
            .collect()
    }
    #[test]
    fn names_spaces_multiple_and_everyone_are_explicit() {
        assert_eq!(
            recipients("@lead and @Claude delegate: review", &peers()),
            Some(vec!["0".into(), "1".into()])
        );
        assert_eq!(recipients("@Lead @everyone", &peers()), Some(vec![]));
        assert_eq!(
            recipients("(@Codex delegate) please", &peers()),
            Some(vec!["2".into()])
        );
    }
    #[test]
    fn emails_links_code_and_partial_names_are_not_addresses() {
        for s in [
            "hello@Lead",
            "https://example.com/@Lead",
            "@Leadership",
            "`@Lead`",
            "```\n@Lead\n```",
            "No mentions",
        ] {
            assert_eq!(recipients(s, &peers()), None, "{s}");
        }
    }
}
