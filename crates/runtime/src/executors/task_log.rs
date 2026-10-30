//! Drain task output without letting it fill the Agent disk or deadlock pipes.
//! The durable prefix is capped; the final bounded tail is carried in the receipt.
use anyhow::Result;
pub use converyn_core::model::logs::{LOG_LIMIT, Logs, Stream};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    os::fd::AsRawFd,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
struct Redactor {
    matcher: Option<aho_corasick::AhoCorasick>,
    longest: usize,
    carry: Vec<u8>,
}
impl Redactor {
    fn new(patterns: Vec<Vec<u8>>) -> Result<Self> {
        let patterns = patterns
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect::<std::collections::BTreeSet<_>>();
        let longest = patterns.iter().map(Vec::len).max().unwrap_or(0);
        let matcher = if patterns.is_empty() {
            None
        } else {
            Some(
                aho_corasick::AhoCorasickBuilder::new()
                    .match_kind(aho_corasick::MatchKind::LeftmostLongest)
                    .build(patterns)?,
            )
        };
        Ok(Self {
            matcher,
            longest,
            carry: Vec::new(),
        })
    }
    fn feed(&mut self, input: &[u8], final_chunk: bool) -> Vec<u8> {
        let Some(matcher) = &self.matcher else {
            return input.to_vec();
        };
        self.carry.extend_from_slice(input);
        let limit = if final_chunk {
            self.carry.len()
        } else {
            self.carry
                .len()
                .saturating_sub(self.longest.saturating_sub(1))
        };
        let mut result = Vec::new();
        let mut consumed = 0;
        for found in matcher.find_iter(&self.carry) {
            if found.start() >= limit {
                break;
            }
            result.extend_from_slice(&self.carry[consumed..found.start()]);
            result.extend_from_slice(b"[REDACTED]");
            consumed = found.end();
        }
        if consumed < limit {
            result.extend_from_slice(&self.carry[consumed..limit]);
            consumed = limit;
        }
        self.carry.drain(..consumed);
        result
    }
}

pub fn capture<R: Read + AsRawFd + Send + 'static>(
    input: R,
    output: File,
    done: Arc<AtomicBool>,
) -> thread::JoinHandle<Result<Stream>> {
    capture_redacted(input, output, done, Vec::new())
}
pub fn capture_redacted<R: Read + AsRawFd + Send + 'static>(
    input: R,
    output: File,
    done: Arc<AtomicBool>,
    secrets: Vec<Vec<u8>>,
) -> thread::JoinHandle<Result<Stream>> {
    capture_prefixed(input, output, done, secrets, Vec::new())
}
pub fn capture_prefixed<R: Read + AsRawFd + Send + 'static>(
    mut input: R,
    mut output: File,
    done: Arc<AtomicBool>,
    secrets: Vec<Vec<u8>>,
    prefix: Vec<u8>,
) -> thread::JoinHandle<Result<Stream>> {
    thread::spawn(move || {
        let mut redactor = Redactor::new(secrets)?;
        output.set_len(0)?;
        let fd = input.as_raw_fd();
        // SAFETY: 操作的是 File 持有的有效 fd；标志与偏移匹配对应命令，返回值会检查。
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        // SAFETY: 操作的是 File 持有的有效 fd；标志与偏移匹配对应命令，返回值会检查。
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut seen = 0u64;
        let mut stored = 0u64;
        let mut hash = Sha256::new();
        let mut tail = Vec::new();
        let mut buf = [0u8; 16384];
        let mut deadline = None;
        let mut incomplete = false;
        let mut append = |bytes: &[u8]| -> Result<()> {
            seen = seen.saturating_add(bytes.len() as u64);
            let keep = bytes.len().min((LOG_LIMIT - stored) as usize);
            output.write_all(&bytes[..keep])?;
            hash.update(&bytes[..keep]);
            stored += keep as u64;
            tail.extend_from_slice(bytes);
            if tail.len() > 4096 {
                tail.drain(..tail.len() - 4096);
            }
            Ok(())
        };
        append(&redactor.feed(&prefix, false))?;
        loop {
            if done.load(Ordering::SeqCst) && deadline.is_none() {
                deadline = Some(Instant::now() + Duration::from_secs(1));
            }
            if deadline.is_some_and(|d| Instant::now() > d) {
                incomplete = true;
                break;
            }
            match input.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => append(&redactor.feed(&buf[..n], false))?,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if deadline.is_some() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }
        append(&redactor.feed(&[], true))?;
        output.sync_all()?;
        let mut text = String::from_utf8_lossy(&tail).into_owned();
        while text.len() > 4096 {
            text.remove(0);
        }
        Ok(Stream {
            stored_bytes: stored,
            seen_bytes: seen,
            sha256: hex::encode(hash.finalize()),
            tail: text,
            truncated: seen > stored || incomplete,
        })
    })
}

/// Redact before JSON escaping: matching raw secrets after escaping can miss
/// quotes and newlines. Omit the whole value whenever it contains a secret.
pub fn environment_prefix(environment: &crate::credentials::secrets::Resolved) -> Vec<u8> {
    if !environment
        .values
        .contains_key(converyn_core::orchestration::values::JSON_ENV)
    {
        return Vec::new();
    }
    let values: std::collections::BTreeMap<_, _> = environment
        .values
        .iter()
        .map(|(name, value)| {
            let secret = environment
                .redactions
                .iter()
                .filter(|s| !s.is_empty())
                .any(|s| value.as_bytes().windows(s.len()).any(|part| part == s));
            (name, if secret { "[REDACTED]" } else { value.as_str() })
        })
        .collect();
    format!(
        "# Converyn effective variables\n{}\n# Script output\n",
        serde_json::to_string(&values).expect("environment is serializable")
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::Redactor;
    #[test]
    fn secrets_spanning_reads_and_overlapping_values_are_redacted() {
        let mut redact = Redactor::new(vec![b"secret".to_vec(), b"secret-long".to_vec()]).unwrap();
        let mut result = Vec::new();
        for bytes in [b"before sec".as_slice(), b"ret-lo", b"ng and secret after"] {
            result.extend(redact.feed(bytes, false));
        }
        result.extend(redact.feed(&[], true));
        assert_eq!(result, b"before [REDACTED] and [REDACTED] after");
    }
}
