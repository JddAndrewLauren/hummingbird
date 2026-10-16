//! #870: every request a sync cycle makes is bounded by
//! `sync::SYNC_REQUEST_TIMEOUT`, so a stalled authority cannot hold the task
//! core. These are the crate's only tests over a real socket: a loopback
//! listener that accepts and never answers (or sends headers and stalls the
//! body) is the one way to make reqwest's own timeout fire. Each transport's
//! bound is shortened with `with_timeout` so a stall costs milliseconds; the
//! production value itself is pinned by the transports' unit tests.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use hummingbird_core::diagnostics::test_support::{RecordingClock, RecordingSink};
use hummingbird_core::diagnostics::{DiagnosticEvent, DiagnosticSession, DiagnosticsContext, FailureClass};
use hummingbird_core::sync::write::ReqwestMutationTransport;
use hummingbird_core::sync::{CycleOutcome, ReqwestSyncTransport, Trigger};
use hummingbird_core::{CaptureOptions, Core, CoreCycleOutcome, CredentialArm};
use hummingbird_domain::{ChangesResponse, Stage};

/// The shortened bound every transport here runs under.
const BOUND: Duration = Duration::from_millis(300);

/// Far under the minutes an OS takes to tear a dead socket down, and far
/// over [`BOUND`]: a request that fails inside this failed *because of* the
/// bound.
const WITHIN_THE_BOUND: Duration = Duration::from_secs(5);

/// How a [`FakeAuthority`] treats every request it accepts.
#[derive(Clone, Copy)]
enum Behaviour {
    /// Accepts the connection, reads the request, never writes a byte.
    Silent,
    /// Sends a 200's headers promising a body, then stalls it after one byte.
    StallsTheBody,
    /// Answers: an empty sweep/delta at `version`, and a create's 201.
    Answers { version: i64 },
}

/// A loopback HTTP/1.1 server, one thread per connection, recording each
/// request's `METHOD path` line.
struct FakeAuthority {
    base_url: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl FakeAuthority {
    fn start(behaviour: Behaviour) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let recorded = Arc::clone(&recorded);
                thread::spawn(move || serve(stream, behaviour, &recorded));
            }
        });
        Self { base_url, requests }
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }

    fn read_transport(&self) -> ReqwestSyncTransport {
        ReqwestSyncTransport::new(reqwest::Client::new(), &self.base_url).with_timeout(BOUND)
    }

    fn write_transport(&self) -> ReqwestMutationTransport {
        ReqwestMutationTransport::new(reqwest::Client::new(), &self.base_url).with_timeout(BOUND)
    }
}

fn serve(mut stream: TcpStream, behaviour: Behaviour, recorded: &Mutex<Vec<String>>) {
    let Some((request_line, body)) = read_request(&mut stream) else { return };
    recorded.lock().unwrap().push(request_line.clone());
    match behaviour {
        Behaviour::Silent => thread::sleep(Duration::from_secs(60)),
        Behaviour::StallsTheBody => {
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 4096\r\n\r\n{",
            );
            let _ = stream.flush();
            thread::sleep(Duration::from_secs(60));
        }
        Behaviour::Answers { version } => {
            let (status, payload) = if request_line.starts_with("POST /api/items") {
                let id = serde_json::from_str::<serde_json::Value>(&body).unwrap()["id"]
                    .as_str()
                    .unwrap()
                    .to_string();
                ("201 Created", format!(r#"{{"id":"{id}","version":{version}}}"#))
            } else {
                ("200 OK", serde_json::to_string(&ChangesResponse::empty(version)).unwrap())
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
                payload.len()
            );
        }
    }
}

/// The request line and body of one request — headers read to the blank
/// line, then `content-length` bytes.
fn read_request(stream: &mut TcpStream) -> Option<(String, String)> {
    let mut buffer = Vec::new();
    let mut byte = [0u8; 1];
    while !buffer.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).ok()? == 0 {
            return None;
        }
        buffer.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&buffer).to_string();
    let content_length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())?
        })
        .unwrap_or(0);
    let mut body = vec![0u8; content_length];
    stream.read_exact(&mut body).ok()?;
    let request_line = head.lines().next()?;
    let mut parts = request_line.split(' ');
    let line = format!("{} {}", parts.next()?, parts.next()?);
    Some((line, String::from_utf8_lossy(&body).to_string()))
}

fn mirror_version(core: &Core) -> i64 {
    core.mirror_snapshot()["version"].as_i64().unwrap()
}

/// Every `http.finished` failure the journal recorded.
fn http_failures(sink: &RecordingSink) -> Vec<Option<FailureClass>> {
    sink.events()
        .into_iter()
        .filter_map(|row| match row.event {
            DiagnosticEvent::HttpFinished { failure, .. } => Some(failure),
            _ => None,
        })
        .collect()
}

/// A core whose mirror has already applied one pull, so "the cursor is
/// unchanged" means something.
async fn core_synced_to(version: i64) -> Core {
    let mut core = Core::new();
    core.push_api_key("device-token");
    let authority = FakeAuthority::start(Behaviour::Answers { version });
    let outcome = core
        .run(&authority.read_transport(), &authority.write_transport(), 1_000, Trigger::User, true, 0.0)
        .await;
    assert!(matches!(outcome, CoreCycleOutcome::Cycle(CycleOutcome::Completed { .. })), "{outcome:?}");
    assert_eq!(mirror_version(&core), version);
    core
}

/// One observed cycle, returning its outcome, how long it took, and the
/// journal it wrote.
async fn observed_cycle(
    core: &mut Core,
    read: &ReqwestSyncTransport,
    write: &ReqwestMutationTransport,
) -> (CoreCycleOutcome, Duration, RecordingSink) {
    let sink = RecordingSink::default();
    let clock = RecordingClock::default();
    let session = DiagnosticSession::new("s-1", 0);
    let diagnostics = DiagnosticsContext::new(&sink, &clock, &session, "c-1", "core", "test", 2_000);
    let started = Instant::now();
    let outcome = core
        .run_observed(read, write, 2_000, Trigger::User, false, 0.0, &diagnostics)
        .await;
    (outcome, started.elapsed(), sink)
}

#[tokio::test]
async fn a_pull_the_authority_never_answers_fails_within_the_bound_with_the_cursor_unchanged() {
    let mut core = core_synced_to(7).await;
    let stalled = FakeAuthority::start(Behaviour::Silent);

    let (outcome, elapsed, sink) =
        observed_cycle(&mut core, &stalled.read_transport(), &stalled.write_transport()).await;

    assert!(elapsed >= BOUND && elapsed < WITHIN_THE_BOUND, "{elapsed:?}");
    assert!(matches!(outcome, CoreCycleOutcome::Cycle(CycleOutcome::PullFailed { .. })), "{outcome:?}");
    assert_eq!(mirror_version(&core), 7, "a timed-out pull never advances the cursor");
    assert_eq!(stalled.requests(), vec!["GET /api/changes?since=7".to_string()]);
    assert_eq!(http_failures(&sink), vec![Some(FailureClass::Timeout)]);
}

#[tokio::test]
async fn a_pull_whose_body_stalls_after_the_headers_fails_within_the_bound_as_a_timeout() {
    let mut core = core_synced_to(7).await;
    let stalled = FakeAuthority::start(Behaviour::StallsTheBody);

    let (outcome, elapsed, sink) =
        observed_cycle(&mut core, &stalled.read_transport(), &stalled.write_transport()).await;

    assert!(elapsed >= BOUND && elapsed < WITHIN_THE_BOUND, "{elapsed:?}");
    assert!(matches!(outcome, CoreCycleOutcome::Cycle(CycleOutcome::PullFailed { .. })), "{outcome:?}");
    assert_eq!(mirror_version(&core), 7);
    assert_eq!(http_failures(&sink), vec![Some(FailureClass::Timeout)]);
}

#[tokio::test]
async fn a_queued_write_that_never_answers_stays_queued_and_drains_exactly_once_next_cycle() {
    let mut core = core_synced_to(7).await;
    core.capture("seed-870", "buy milk", Stage::Triage, 1_500, CaptureOptions::default(), None)
        .await
        .unwrap();
    assert_eq!(core.queue_depth(), 1);
    let stalled = FakeAuthority::start(Behaviour::Silent);

    let (outcome, elapsed, sink) =
        observed_cycle(&mut core, &stalled.read_transport(), &stalled.write_transport()).await;

    assert!(elapsed >= BOUND && elapsed < WITHIN_THE_BOUND, "{elapsed:?}");
    assert!(matches!(outcome, CoreCycleOutcome::Cycle(CycleOutcome::Blocked { .. })), "{outcome:?}");
    assert_eq!(core.queue_depth(), 1, "the timed-out write is still queued");
    assert!(core.dead_letters().is_empty(), "a timeout is not a reason to give a write up");
    assert_eq!(http_failures(&sink), vec![Some(FailureClass::Timeout)]);

    let answering = FakeAuthority::start(Behaviour::Answers { version: 8 });
    let (outcome, _, _) =
        observed_cycle(&mut core, &answering.read_transport(), &answering.write_transport()).await;

    assert!(matches!(outcome, CoreCycleOutcome::Cycle(CycleOutcome::Completed { .. })), "{outcome:?}");
    assert_eq!(core.queue_depth(), 0);
    let creates = answering
        .requests()
        .into_iter()
        .filter(|line| line == "POST /api/items")
        .count();
    assert_eq!(creates, 1, "the queued write drains exactly once");
}

#[tokio::test]
async fn a_timed_out_cycle_leaves_the_credential_resting() {
    let mut core = core_synced_to(7).await;
    core.take_events();
    let stalled = FakeAuthority::start(Behaviour::Silent);

    observed_cycle(&mut core, &stalled.read_transport(), &stalled.write_transport()).await;

    assert_eq!(core.credential_arm(), CredentialArm::Resting);
    assert!(core.take_events().is_empty(), "no credential change is reported for a timeout");

    let answering = FakeAuthority::start(Behaviour::Answers { version: 8 });
    let (outcome, _, _) =
        observed_cycle(&mut core, &answering.read_transport(), &answering.write_transport()).await;
    assert!(matches!(outcome, CoreCycleOutcome::Cycle(CycleOutcome::Completed { .. })), "{outcome:?}");
}
