//! Background HTTP fetches for extensions.
//!
//! A [`Fetch`] runs one GET request on its own thread; the caller polls it every
//! frame and never blocks, so a slow or absent network cannot stall logging.
//! Extensions start a fetch only on an explicit action of the operator (opening a
//! panel, pressing a button), never on their own at start-up.

use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::Duration;

/// User-Agent sent with every request; some services require one.
pub const USER_AGENT: &str = concat!("zlog-rs/", env!("CARGO_PKG_VERSION"));

/// User-Agent of a common browser, for services that refuse any other
/// (the radio station search of the MIC answers 403 to `zlog-rs/...`).
pub const BROWSER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

/// One GET request in flight, or its result.
pub struct Fetch {
    rx: Receiver<Result<String, String>>,
    done: Option<Result<String, String>>,
}

impl Fetch {
    /// Start fetching `url` as text.
    pub fn get(url: impl Into<String>) -> Self {
        Fetch::get_as(url, USER_AGENT)
    }

    /// Start fetching `url` as text, sending `user_agent` as the User-Agent.
    pub fn get_as(url: impl Into<String>, user_agent: &'static str) -> Self {
        let url = url.into();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(fetch(&url, user_agent));
        });
        Fetch { rx, done: None }
    }

    /// The result once it has arrived; `None` while the request is in flight.
    pub fn poll(&mut self) -> Option<&Result<String, String>> {
        if self.done.is_none() {
            match self.rx.try_recv() {
                Ok(r) => self.done = Some(r),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.done = Some(Err("request thread ended".into()))
                }
            }
        }
        self.done.as_ref()
    }
}

fn fetch(url: &str, user_agent: &str) -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into();
    agent
        .get(url)
        .header("User-Agent", user_agent)
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())
}

/// Download `url` into `out` on the calling thread (run it on a thread of your own),
/// calling `progress(received, total)` as bytes arrive; `total` is the
/// Content-Length when the server sends one. For files too large for [`Fetch`].
pub fn download(
    url: &str,
    out: &mut impl std::io::Write,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<(), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .build()
        .into();
    let mut response = agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| e.to_string())?;
    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());
    let mut reader = response.body_mut().with_config().limit(u64::MAX).reader();
    let mut buf = vec![0u8; 64 * 1024];
    let mut received = 0u64;
    loop {
        let n = std::io::Read::read(&mut reader, &mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        received += n as u64;
        progress(received, total);
    }
    out.flush().map_err(|e| e.to_string())
}

/// Strip HTML tags and decode the few entities that matter for table text.
pub fn html_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_removed() {
        assert_eq!(html_text("<td><P align=center> 11.5</td>"), " 11.5");
        assert_eq!(html_text("a&amp;b"), "a&b");
    }
}
