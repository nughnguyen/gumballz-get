//! POSIX-style tokenizing and quoting for the yt-dlp command.
//!
//! It exists because the screen shows the exact command of each attempt and lets the
//! user edit it and run it again. The outbound path (argv → text) must
//! be reversible by the inbound path (text → argv); keeping both here,
//! with round-trip tests, is what guarantees that. We do not use `sh -c`: the
//! text becomes argv and goes straight to the process, with no shell in between.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    UnclosedQuote(char),
    TrailingBackslash,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::UnclosedQuote(q) => write!(f, "unclosed quote: {q}"),
            ParseError::TrailingBackslash => write!(f, "trailing backslash"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Splits `input` into arguments: literal single quotes, double quotes with
/// `\"` and `\\`, a backslash outside quotes escapes the next character,
/// whitespace separates. A line break counts as a space, so the user
/// can format the command across several lines in the edit box.
pub fn split(input: &str) -> Result<Vec<String>, ParseError> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_token = false;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_token = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(ch) => cur.push(ch),
                        None => return Err(ParseError::UnclosedQuote('\'')),
                    }
                }
            }
            '"' => {
                in_token = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(esc @ ('"' | '\\' | '$' | '`')) => cur.push(esc),
                            Some(other) => {
                                cur.push('\\');
                                cur.push(other);
                            }
                            None => return Err(ParseError::UnclosedQuote('"')),
                        },
                        Some(ch) => cur.push(ch),
                        None => return Err(ParseError::UnclosedQuote('"')),
                    }
                }
            }
            '\\' => match chars.next() {
                Some('\n') => {} // line continuation: does not open a token
                Some(ch) => {
                    in_token = true;
                    cur.push(ch);
                }
                None => return Err(ParseError::TrailingBackslash),
            },
            c if c.is_whitespace() => {
                if in_token {
                    out.push(std::mem::take(&mut cur));
                    in_token = false;
                }
            }
            other => {
                in_token = true;
                cur.push(other);
            }
        }
    }
    if in_token {
        out.push(cur);
    }
    Ok(out)
}

fn needs_quoting(arg: &str) -> bool {
    arg.is_empty()
        || arg.chars().any(|c| {
            c.is_whitespace()
                || matches!(
                    c,
                    '\'' | '"'
                        | '\\'
                        | '$'
                        | '`'
                        | '!'
                        | '*'
                        | '?'
                        | '['
                        | ']'
                        | '('
                        | ')'
                        | '{'
                        | '}'
                        | '<'
                        | '>'
                        | '|'
                        | '&'
                        | ';'
                        | '#'
                        | '~'
                )
        })
}

/// Quotes an argument for display/editing. Prefers single quotes (no escapes
/// inside); only falls back to double quotes when the text contains single quotes.
pub fn quote(arg: &str) -> String {
    if !needs_quoting(arg) {
        return arg.to_string();
    }
    if !arg.contains('\'') {
        return format!("'{arg}'");
    }
    let mut s = String::with_capacity(arg.len() + 2);
    s.push('"');
    for c in arg.chars() {
        if matches!(c, '"' | '\\' | '$' | '`') {
            s.push('\\');
        }
        s.push(c);
    }
    s.push('"');
    s
}

pub fn join<I, S>(args: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .map(|a| quote(a.as_ref()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Flags whose value is a secret. The value is swapped for `<redacted>` before going
/// to the log and the screen; the list is short on purpose (showing too much is
/// less dangerous than hiding what the user needs to edit).
const SECRET_VALUE_FLAGS: &[&str] = &[
    "--password",
    "--video-password",
    "--ap-password",
    "--client-certificate-password",
    "--proxy",
    "--all-proxy",
];

/// Returns a copy of the argv with secrets redacted: `Cookie:` header,
/// passwords, proxy with credentials, and `po_token=` inside `--extractor-args`.
pub fn redact(args: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut redact_next = false;
    let mut header_next = false;
    let mut extractor_next = false;
    for a in args {
        if redact_next {
            redact_next = false;
            out.push(redact_proxy_or_secret(a));
            continue;
        }
        if header_next {
            header_next = false;
            out.push(redact_header_value(a));
            continue;
        }
        if extractor_next {
            extractor_next = false;
            out.push(redact_extractor_args(a));
            continue;
        }
        if SECRET_VALUE_FLAGS.contains(&a.as_str()) {
            redact_next = true;
            out.push(a.clone());
            continue;
        }
        if a == "--add-headers" || a == "--add-header" {
            header_next = true;
            out.push(a.clone());
            continue;
        }
        if a == "--extractor-args" {
            extractor_next = true;
            out.push(a.clone());
            continue;
        }
        if let Some((flag, value)) = a.split_once('=') {
            if SECRET_VALUE_FLAGS.contains(&flag) {
                out.push(format!("{flag}={}", redact_proxy_or_secret(value)));
                continue;
            }
            if flag == "--add-headers" || flag == "--add-header" {
                out.push(format!("{flag}={}", redact_header_value(value)));
                continue;
            }
            if flag == "--extractor-args" {
                out.push(format!("{flag}={}", redact_extractor_args(value)));
                continue;
            }
        }
        out.push(a.clone());
    }
    out
}

fn redact_header_value(v: &str) -> String {
    match v.split_once(':') {
        Some((name, _)) if name.trim().eq_ignore_ascii_case("cookie") => {
            format!("{}:<redacted>", name.trim())
        }
        Some((name, _)) if name.trim().eq_ignore_ascii_case("authorization") => {
            format!("{}:<redacted>", name.trim())
        }
        _ => v.to_string(),
    }
}

fn redact_proxy_or_secret(v: &str) -> String {
    // a proxy is only a secret when it carries user:password
    if let Ok(mut url) = url::Url::parse(v) {
        if url.username().is_empty() && url.password().is_none() {
            return v.to_string();
        }
        let _ = url.set_username("<redacted>");
        let _ = url.set_password(None);
        return url.to_string();
    }
    "<redacted>".to_string()
}

fn redact_extractor_args(v: &str) -> String {
    if !v.contains("po_token") {
        return v.to_string();
    }
    v.split(';')
        .map(|part| {
            if part.trim_start().starts_with("po_token") {
                "po_token=<redacted>".to_string()
            } else {
                part.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_spaces_and_quotes() {
        let v = split(r#"yt-dlp -f 'bv*+ba' --add-headers "Referer: https://x/" url"#).unwrap();
        assert_eq!(
            v,
            vec![
                "yt-dlp",
                "-f",
                "bv*+ba",
                "--add-headers",
                "Referer: https://x/",
                "url"
            ]
        );
    }

    #[test]
    fn backslash_escapes_and_continues_line() {
        let v = split("a\\ b \\\n c").unwrap();
        assert_eq!(v, vec!["a b", "c"]);
    }

    #[test]
    fn error_on_open_quotes() {
        assert_eq!(split("a 'b"), Err(ParseError::UnclosedQuote('\'')));
        assert_eq!(split("a \"b"), Err(ParseError::UnclosedQuote('"')));
        assert_eq!(split("a\\"), Err(ParseError::TrailingBackslash));
    }

    #[test]
    fn round_trip_preserves_argv() {
        // The displayed command must come back identical when the user just hits
        // "run as-is"; any difference here would become a silent bug.
        let argv: Vec<String> = [
            "yt-dlp",
            "-f",
            "bv*[height<=1080]+ba[ext=m4a]/b",
            "-o",
            "/Users/x/Downloads/%(title).200s [%(id)s].%(ext)s",
            "--progress-template",
            "download:%(progress._percent_str)s|eta:%(progress.eta)s",
            "it's",
            "",
            "--extractor-args",
            "youtube:player_client=default;formats=dashy",
            "https://www.youtube.com/watch?v=abc",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let text = join(&argv);
        assert_eq!(split(&text).unwrap(), argv, "{text}");
    }

    #[test]
    fn redacts_cookie_password_proxy_and_po_token() {
        let argv: Vec<String> = [
            "--add-headers",
            "Cookie:SID=abc",
            "--add-headers",
            "Referer:https://a/",
            "--password",
            "hunter2",
            "--proxy",
            "http://user:pw@host:1/",
            "--proxy",
            "http://host:1/",
            "--extractor-args",
            "youtube:player_client=web;po_token=web.gvs+XYZ",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let r = redact(&argv);
        assert_eq!(r[1], "Cookie:<redacted>");
        assert_eq!(r[3], "Referer:https://a/");
        assert_eq!(r[5], "<redacted>");
        assert_eq!(r[7], "http://%3Credacted%3E@host:1/");
        assert_eq!(r[9], "http://host:1/");
        assert_eq!(r[11], "youtube:player_client=web;po_token=<redacted>");
    }
}
