//! Black box: the app's last N events, redacted, to attach to a bug report.
//!
//! The `download_log` that already exists is per download and disappears on
//! restart. This one is global and spans downloads, plugins and bootstrap —
//! what is missing when someone says "it stopped working" and cannot say when
//! it started.
//!
//! The point here is the **redaction**, not the buffer. A log dump that leaks a
//! cookie or token is worse than no dump, because the user pastes it into a
//! public issue thinking it is safe. That is why redaction is a pure function
//! tested per secret class, and the buffer is just a `VecDeque`.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

/// How many events are retained. High enough to cover a download session, low
/// enough to fit in an issue comment.
pub const CAPACITY: usize = 500;

/// Replaces every known secret with a stable marker.
///
/// A blocklist is the opposite of what security wants, but here it is
/// unavoidable: the input is arbitrary log from yt-dlp, ffmpeg and plugins, and
/// there is no way to enumerate what is safe. The mitigation is to cover by
/// *class* of secret and test each one, instead of by literal pattern.
pub fn redact(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;

    while !rest.is_empty() {
        match next_secret(rest) {
            Some((start, len, replacement)) => {
                out.push_str(&rest[..start]);
                out.push_str(replacement);
                rest = &rest[start + len..];
            }
            None => {
                out.push_str(rest);
                break;
            }
        }
    }

    redact_home_paths(&out)
}

/// Finds the next secret: `(offset, length, replacement)`.
fn next_secret(hay: &str) -> Option<(usize, usize, &'static str)> {
    let lower = hay.to_lowercase();
    let mut best: Option<(usize, usize, &'static str)> = None;

    // Key=value, in a query string, header or config line.
    const KEYED: &[&str] = &[
        "cookie=",
        "cookies=",
        "token=",
        "access_token=",
        "refresh_token=",
        "api_key=",
        "apikey=",
        "key=",
        "password=",
        "passwd=",
        "secret=",
        "authorization=",
        "sessdata=",
        "bili_jct=",
        "auth=",
        "sig=",
        "signature=",
    ];
    for k in KEYED {
        if let Some(i) = lower.find(k) {
            let value_start = i + k.len();
            let value_len = hay[value_start..]
                .find(|c: char| c.is_whitespace() || c == '&' || c == ';' || c == '"' || c == '\'')
                .unwrap_or(hay.len() - value_start);
            if value_len > 0 {
                let cand = (value_start, value_len, "[REDACTED]");
                if best.is_none_or(|(b, _, _)| cand.0 < b) {
                    best = Some(cand);
                }
            }
        }
    }

    // Header `Authorization: Bearer <token>`.
    if let Some(i) = lower.find("bearer ") {
        let value_start = i + "bearer ".len();
        let value_len = hay[value_start..]
            .find(char::is_whitespace)
            .unwrap_or(hay.len() - value_start);
        if value_len > 0 {
            let cand = (value_start, value_len, "[REDACTED]");
            if best.is_none_or(|(b, _, _)| cand.0 < b) {
                best = Some(cand);
            }
        }
    }

    best
}

/// Swaps the home directory for `~`, in all three conventions.
///
/// A username in a path is identifying and leaks on its own — many people paste
/// the log without looking.
fn redact_home_paths(line: &str) -> String {
    let mut out = line.to_string();
    for prefix in ["/Users/", "/home/", "\\Users\\"] {
        while let Some(i) = out.find(prefix) {
            let after = i + prefix.len();
            let sep = if prefix.contains('\\') { '\\' } else { '/' };
            let end = out[after..]
                .find(|c: char| c == sep || c.is_whitespace())
                .map(|e| after + e)
                .unwrap_or(out.len());
            if end == after {
                break; // prefix with no username after it; nothing to redact
            }
            out.replace_range(i..end, "~");
        }
    }
    out
}

/// Circular buffer, with no global state.
///
/// Extracted on purpose: with the buffer behind a `static`, two tests that
/// record events fight over the same structure and one of them fails
/// intermittently. Testing the structure directly removes the race instead of
/// working around it with test serialization.
#[derive(Debug)]
pub struct Recorder {
    events: VecDeque<String>,
    capacity: usize,
}

impl Recorder {
    pub fn new(capacity: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(capacity.min(1024)),
            capacity: capacity.max(1),
        }
    }

    /// Records an event. Redaction happens **at entry**, not at dump time: a
    /// secret that comes in in the clear has already leaked to anyone with
    /// access to memory, and the dump can be forgotten.
    pub fn record(&mut self, event: &str) {
        if self.events.len() >= self.capacity {
            self.events.pop_front();
        }
        self.events.push_back(redact(event));
    }

    /// From oldest to newest.
    pub fn dump(&self) -> Vec<String> {
        self.events.iter().cloned().collect()
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

static BUFFER: OnceLock<Mutex<Recorder>> = OnceLock::new();

fn buffer() -> &'static Mutex<Recorder> {
    BUFFER.get_or_init(|| Mutex::new(Recorder::new(CAPACITY)))
}

pub fn record(event: &str) {
    if let Ok(mut buf) = buffer().lock() {
        buf.record(event);
    }
}

pub fn dump() -> Vec<String> {
    buffer().lock().map(|b| b.dump()).unwrap_or_default()
}

pub fn clear() {
    if let Ok(mut buf) = buffer().lock() {
        buf.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redige_cookie_em_query_string() {
        let s = redact("GET /video?id=42&cookie=abc123def&x=1");
        assert!(!s.contains("abc123def"), "{s}");
        assert!(s.contains("cookie=[REDACTED]"), "{s}");
        // The rest of the line has to survive, or the log loses its usefulness.
        assert!(s.contains("id=42"), "{s}");
        assert!(s.contains("x=1"), "{s}");
    }

    #[test]
    fn redige_cada_classe_de_segredo() {
        let casos = [
            ("SESSDATA=deadbeef", "deadbeef"),
            ("bili_jct=abc99", "abc99"),
            ("Authorization: Bearer eyJhbGciOi", "eyJhbGciOi"),
            ("--api_key=sk-live-1234", "sk-live-1234"),
            ("password=hunter2", "hunter2"),
            ("access_token=ya29.a0Af", "ya29.a0Af"),
            ("?sig=9f8e7d6c", "9f8e7d6c"),
        ];
        for (entrada, segredo) in casos {
            let s = redact(entrada);
            assert!(!s.contains(segredo), "leaked {segredo:?} in {s:?}");
            assert!(s.contains("[REDACTED]"), "{s}");
        }
    }

    #[test]
    fn redige_o_nome_de_usuario_do_caminho() {
        // A personal path identifies the person on its own, and it is what shows
        // up most in logs pasted into public issues.
        for (entrada, proibido) in [
            ("/Users/nughnguyen/Downloads/x.mp4", "nughnguyen"),
            ("/home/maria/videos", "maria"),
            ("C:\\Users\\Joao\\AppData", "Joao"),
        ] {
            let s = redact(entrada);
            assert!(!s.contains(proibido), "leaked username in {s:?}");
            assert!(s.contains('~'), "{s}");
        }
    }

    #[test]
    fn linha_sem_segredo_atravessa_intacta() {
        // Redaction that is too aggressive destroys the log's value.
        let limpa = "[ytdlp] merging formats 137+140 into out.mp4";
        assert_eq!(redact(limpa), limpa);
    }

    #[test]
    fn multiplos_segredos_na_mesma_linha() {
        let s = redact("cookie=aaa&token=bbb&keep=ok");
        assert!(!s.contains("aaa") && !s.contains("bbb"), "{s}");
        assert!(s.contains("keep=ok"), "{s}");
        assert_eq!(s.matches("[REDACTED]").count(), 2, "{s}");
    }

    #[test]
    fn o_buffer_descarta_o_mais_antigo_e_preserva_a_ordem() {
        let mut r = Recorder::new(5);
        for i in 0..15 {
            r.record(&format!("evento {i}"));
        }
        let d = r.dump();
        assert_eq!(d.len(), 5);
        assert_eq!(d.first().unwrap(), "evento 10");
        assert_eq!(d.last().unwrap(), "evento 14");
    }

    #[test]
    fn o_buffer_guarda_redigido_e_nao_o_original() {
        // Redacting only at dump time would leave the secret in memory and depend
        // on nobody forgetting to call redaction on the way out.
        let mut r = Recorder::new(4);
        r.record("cookie=segredo_absoluto");
        assert!(!r.dump()[0].contains("segredo_absoluto"), "{:?}", r.dump());
    }

    #[test]
    fn capacidade_zero_nao_vira_buffer_que_engole_tudo() {
        let mut r = Recorder::new(0);
        r.record("a");
        r.record("b");
        assert_eq!(r.len(), 1);
        assert_eq!(r.dump(), vec!["b"]);
    }
}
