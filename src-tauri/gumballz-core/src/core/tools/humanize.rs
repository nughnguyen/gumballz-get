//! Humanize text: applies the "Humanizer" skill (blader/humanizer, MIT) to the
//! configured AI. The whole skill goes in the system prompt; the user's text
//! goes as the message. Only the final text comes back, no draft or
//! critique, because there is no chat here to discuss the result.

use crate::core::ai;

/// The original SKILL.md, without the frontmatter. Lives in a `.txt` because the repo
/// does not version `.md`.
pub const SKILL: &str = include_str!("humanizer_skill.txt");

/// Generous limit: above it the model loses the thread and the cost spikes.
pub const MAX_CHARS: usize = 40_000;

pub fn system_prompt(sample: Option<&str>) -> String {
    let mut s = String::with_capacity(SKILL.len() + 1024);
    s.push_str(SKILL);
    s.push_str(
        "\n\n## Embedded mode (this run)\n\
         You are running in embedded mode inside a desktop app. Return ONLY the final rewritten text: \
         no draft, no list of patterns, no preamble, no closing remark, no code fences around the text. \
         Keep the language of the input (Portuguese stays Portuguese, English stays English). \
         Keep code blocks, URLs, numbers, names, quotes and data exactly as they are. \
         Keep the original formatting (paragraphs, lists, headings) unless a pattern above says otherwise.\n",
    );
    if let Some(sample) = sample.map(str::trim).filter(|s| !s.is_empty()) {
        s.push_str("\n## Writing sample from the user (match this voice)\n\n");
        s.push_str(sample);
        s.push('\n');
    }
    s
}

pub async fn humanize(text: &str, sample: Option<&str>) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("empty text".into());
    }
    if text.chars().count() > MAX_CHARS {
        return Err(format!(
            "text too large ({} characters; the limit is {})",
            text.chars().count(),
            MAX_CHARS
        ));
    }
    if !ai::get().is_configured() {
        return Err("ai_not_configured".into());
    }
    let out = ai::chat(&system_prompt(sample), text).await?;
    Ok(strip_fence(out.trim()).to_string())
}

/// Some models return the text inside ```…``` even when asked not to.
fn strip_fence(s: &str) -> &str {
    let t = s.trim();
    if let Some(inner) = t.strip_prefix("```") {
        if let Some(end) = inner.rfind("```") {
            let body = &inner[..end];
            // skip the language identifier on the first line, if any
            return body
                .split_once('\n')
                .map(|(_, rest)| rest)
                .unwrap_or(body)
                .trim();
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_contains_skill_and_embedded_mode() {
        let p = system_prompt(Some("oi, sou eu"));
        assert!(p.contains("Signs of AI writing"));
        assert!(p.contains("Embedded mode"));
        assert!(p.contains("oi, sou eu"));
        assert!(!system_prompt(None).contains("Writing sample"));
    }

    #[test]
    fn strips_code_fence() {
        assert_eq!(strip_fence("```text\nolá\n```"), "olá");
        assert_eq!(strip_fence("```\nolá\n```"), "olá");
        assert_eq!(strip_fence("olá"), "olá");
    }
}
