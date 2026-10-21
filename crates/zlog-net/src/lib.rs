//! Background HTTP fetches for extensions.
//!
//! A [`Fetch`] runs one GET request on its own thread; the caller polls it every
//! frame and never blocks, so a slow or absent network cannot stall logging.
//! Extensions start a fetch only on an explicit action of the operator (opening a
//! panel, pressing a button), never on their own at start-up.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::Duration;

/// Set by [`set_offline`]: every request fails at once without touching the network.
static OFFLINE: AtomicBool = AtomicBool::new(false);

/// The answer of every request while offline.
const OFFLINE_ERROR: &str = "offline";

/// Make every later request fail at once with an error, without reaching the
/// network (for tests of programs that use this crate).
pub fn set_offline(offline: bool) {
    OFFLINE.store(offline, Ordering::Relaxed);
}

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
    if OFFLINE.load(Ordering::Relaxed) {
        return Err(OFFLINE_ERROR.into());
    }
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
    progress: impl FnMut(u64, Option<u64>),
) -> Result<(), String> {
    download_within(url, out, progress, DOWNLOAD_TIMEOUT)
}

/// Longest a [`download`] may take in all, so a server that stops sending mid-way
/// cannot hang it; generous for the files downloaded (zLog updates, lists).
pub const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(5 * 60);

fn download_within(
    url: &str,
    out: &mut impl std::io::Write,
    mut progress: impl FnMut(u64, Option<u64>),
    timeout: Duration,
) -> Result<(), String> {
    if OFFLINE.load(Ordering::Relaxed) {
        return Err(OFFLINE_ERROR.into());
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_global(Some(timeout))
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
    // `&amp;` last, so that an escaped entity (`&amp;lt;`) stays the text `&lt;`.
    out.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_requests_fail_at_once() {
        set_offline(true);
        let mut f = Fetch::get("http://192.0.2.1/");
        let started = std::time::Instant::now();
        let r = loop {
            if let Some(r) = f.poll() {
                break r.clone();
            }
            std::thread::yield_now();
        };
        set_offline(false);
        assert_eq!(r, Err(OFFLINE_ERROR.to_string()));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn tags_are_removed() {
        assert_eq!(html_text("<td><P align=center> 11.5</td>"), " 11.5");
        assert_eq!(html_text("a&amp;b"), "a&b");
        assert_eq!(html_text("a&amp;lt;b&amp;gt;"), "a&lt;b&gt;");
    }

    #[test]
    fn a_stalled_download_times_out() {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        // Headers and part of the body, then silence with the connection open.
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = std::io::Read::read(&mut s, &mut buf);
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nabc")
                .unwrap();
            std::thread::sleep(Duration::from_secs(10));
        });
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut out = Vec::new();
            let r = download_within(
                &format!("http://{addr}/"),
                &mut out,
                |_, _| {},
                Duration::from_secs(1),
            );
            let _ = tx.send(r);
        });
        let r = rx
            .recv_timeout(Duration::from_secs(8))
            .expect("download hung");
        assert!(r.is_err());
        drop(server);
    }
}
