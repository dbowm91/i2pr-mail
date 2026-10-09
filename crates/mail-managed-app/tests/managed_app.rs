//! Deterministic fake-host integration tests for the managed-app client.
//!
//! Every test uses injected duplex I/O; no router, network, or filesystem is
//! touched. The fake host speaks the same frozen wire contract from the
//! opposite direction and is deliberately adversarial where the plan requires
//! fail-closed evidence.
//!
//! All integration tests run on a multi-thread runtime. The client spawns
//! single-owner reader/writer tasks; multi-thread scheduling keeps the test
//! harness deterministic while the product code itself uses only bounded
//! channels and explicit shutdown signalling.

use i2pr_mail_managed_app::{
    AppService, ManagedAppClient, ManagedLaunchIdentity,
    client::ManagedError,
    control::{AppRequest, HostEvent},
    frame::{FRAME_HEADER_LEN, KIND_CONTROL, KIND_DATA, decode_header, encode_header},
    handshake::decode_handshake,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn test_identity() -> ManagedLaunchIdentity {
    ManagedLaunchIdentity {
        app_id: "mail/test-client".to_string(),
        instance_id: 7,
        instance_decimal: "7".to_string(),
    }
}

async fn write_frame<T>(io: &mut T, kind: u8, stream_id: u32, payload: &[u8])
where
    T: AsyncWriteExt + Unpin,
{
    let header = encode_header(kind, stream_id, payload.len()).expect("valid test frame");
    io.write_all(&header).await.expect("host header");
    io.write_all(payload).await.expect("host payload");
    io.flush().await.expect("host flush");
}

async fn read_frame<T>(io: &mut T) -> (u8, u32, Vec<u8>)
where
    T: AsyncReadExt + Unpin,
{
    let mut hb = [0u8; FRAME_HEADER_LEN];
    io.read_exact(&mut hb).await.expect("read header");
    let h = decode_header(&hb).expect("valid header");
    let mut payload = vec![0u8; h.payload_len];
    if !payload.is_empty() {
        io.read_exact(&mut payload).await.expect("read payload");
    }
    (h.kind, h.stream_id, payload)
}

async fn host_establish<T>(io: &mut T, granted: Vec<String>)
where
    T: AsyncReadExt + AsyncWriteExt + Unpin,
{
    let mut hb = [0u8; 8];
    io.read_exact(&mut hb).await.expect("host handshake");
    decode_handshake(&hb).expect("valid handshake");
    let (kind, sid, payload) = read_frame(io).await;
    assert_eq!(kind, KIND_CONTROL);
    assert_eq!(sid, 0);
    assert!(matches!(
        AppRequest::decode(&payload).expect("hello"),
        AppRequest::Hello { id: 1, .. }
    ));
    let reply = HostEvent::Reply {
        id: 1,
        ok: true,
        error: None,
    };
    write_frame(io, KIND_CONTROL, 0, &reply.encode()).await;
    let caps = HostEvent::Capabilities { granted };
    write_frame(io, KIND_CONTROL, 0, &caps.encode()).await;
}

fn payload_close_id(payload: &[u8]) -> u32 {
    match AppRequest::decode(payload).expect("close decodes") {
        AppRequest::Close { stream_id } => stream_id,
        other => panic!("expected close, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hello_and_capabilities_establish_and_freeze() {
    let (client_io, mut host_io) = tokio::io::duplex(256 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io, vec!["sam".to_string()]).await;
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("establish");
    assert_eq!(client.capabilities(), vec!["sam".to_string()]);
    assert!(!client.is_closed());
    client.shutdown();
    host.await.expect("host");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn open_sam_yields_isolated_echo_stream() {
    let (client_io, mut host_io) = tokio::io::duplex(256 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io, vec!["sam".to_string()]).await;
        let (_, _, payload) = read_frame(&mut host_io).await;
        let stream_id = match AppRequest::decode(&payload).expect("open") {
            AppRequest::Open {
                id: 2,
                service,
                stream_id,
            } => {
                assert_eq!(service, "sam");
                stream_id
            }
            other => panic!("expected open id 2, got {other:?}"),
        };
        let ok = HostEvent::Reply {
            id: 2,
            ok: true,
            error: None,
        };
        write_frame(&mut host_io, KIND_CONTROL, 0, &ok.encode()).await;
        let (kind, sid, data) = read_frame(&mut host_io).await;
        assert_eq!(kind, KIND_DATA);
        assert_eq!(sid, stream_id);
        assert_eq!(data, b"hello-mail");
        write_frame(&mut host_io, KIND_DATA, stream_id, b"hello-mail").await;
        let (_, _, payload) = read_frame(&mut host_io).await;
        assert_eq!(payload_close_id(&payload), stream_id);
        let closed = HostEvent::StreamClosed { stream_id };
        write_frame(&mut host_io, KIND_CONTROL, 0, &closed.encode()).await;
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("connect");
    let mut stream = client.open_service(AppService::Sam).await.expect("open");
    assert_eq!(stream.stream_id(), 1);
    stream.write_all(b"hello-mail").await.expect("write");
    stream.flush().await.expect("flush");
    let mut buf = vec![0u8; 32];
    let n = stream.read(&mut buf).await.expect("read");
    assert_eq!(&buf[..n], b"hello-mail");
    drop(stream);
    host.await.expect("host");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn open_without_grant_denied_before_id_allocation() {
    let (client_io, mut host_io) = tokio::io::duplex(256 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io, vec![]).await;
        let res = tokio::time::timeout(
            tokio::time::Duration::from_millis(150),
            read_frame(&mut host_io),
        )
        .await;
        assert!(res.is_err(), "denied open must emit no frame");
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("connect");
    let err = match client.open_service(AppService::Sam).await {
        Ok(_) => panic!("expected denied"),
        Err(e) => e,
    };
    assert_eq!(err, ManagedError::Denied);
    host.await.expect("host");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wrong_hello_reply_fails_closed() {
    let (client_io, mut host_io) = tokio::io::duplex(64 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        let mut hb = [0u8; 8];
        host_io.read_exact(&mut hb).await.expect("handshake");
        let (_, _, _) = read_frame(&mut host_io).await;
        let bad = HostEvent::Reply {
            id: 99,
            ok: true,
            error: None,
        };
        write_frame(&mut host_io, KIND_CONTROL, 0, &bad.encode()).await;
    });
    let err = match ManagedAppClient::connect(client_io, &identity).await {
        Ok(_) => panic!("expected hello failure"),
        Err(e) => e,
    };
    assert_eq!(err, ManagedError::HelloFailed);
    host.await.expect("host");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hello_reply_failure_is_hello_failed() {
    let (client_io, mut host_io) = tokio::io::duplex(64 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        let mut hb = [0u8; 8];
        host_io.read_exact(&mut hb).await.expect("handshake");
        let (_, _, _) = read_frame(&mut host_io).await;
        let fail = HostEvent::Reply {
            id: 1,
            ok: false,
            error: Some("denied".to_string()),
        };
        write_frame(&mut host_io, KIND_CONTROL, 0, &fail.encode()).await;
    });
    let err = match ManagedAppClient::connect(client_io, &identity).await {
        Ok(_) => panic!("expected failure"),
        Err(e) => e,
    };
    assert_eq!(err, ManagedError::HelloFailed);
    host.await.expect("host");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn missing_capabilities_event_fails() {
    let (client_io, mut host_io) = tokio::io::duplex(64 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        let mut hb = [0u8; 8];
        host_io.read_exact(&mut hb).await.expect("handshake");
        let (_, _, _) = read_frame(&mut host_io).await;
        let reply = HostEvent::Reply {
            id: 1,
            ok: true,
            error: None,
        };
        write_frame(&mut host_io, KIND_CONTROL, 0, &reply.encode()).await;
        // Wrong event instead of capabilities.
        let health = HostEvent::Health {
            status: "ok".to_string(),
        };
        write_frame(&mut host_io, KIND_CONTROL, 0, &health.encode()).await;
    });
    let err = match ManagedAppClient::connect(client_io, &identity).await {
        Ok(_) => panic!("expected caps failure"),
        Err(e) => e,
    };
    assert_eq!(err, ManagedError::CapabilitiesFailed);
    host.await.expect("host");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn duplicate_capabilities_fatal_closes_channel() {
    let (client_io, mut host_io) = tokio::io::duplex(256 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io, vec!["sam".to_string()]).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let caps = HostEvent::Capabilities {
            granted: vec!["sam".to_string()],
        };
        write_frame(&mut host_io, KIND_CONTROL, 0, &caps.encode()).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("connect");
    assert!(!client.is_closed());
    host.await.expect("host");
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    assert!(client.is_closed());
    let err = match client.open_service(AppService::Sam).await {
        Ok(_) => panic!("expected closed"),
        Err(e) => e,
    };
    assert_eq!(err, ManagedError::ChannelClosed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn close_produces_eof_and_reset_produces_reset_error() {
    // Close -> EOF (with data sync proving the stream was usable first).
    let (client_io, mut host_io) = tokio::io::duplex(256 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io, vec!["sam".to_string()]).await;
        let (_, _, payload) = read_frame(&mut host_io).await;
        let sid = match AppRequest::decode(&payload).expect("open") {
            AppRequest::Open { stream_id, .. } => stream_id,
            other => panic!("open {other:?}"),
        };
        let ok = HostEvent::Reply {
            id: 2,
            ok: true,
            error: None,
        };
        write_frame(&mut host_io, KIND_CONTROL, 0, &ok.encode()).await;
        let (kind, data_sid, data) = read_frame(&mut host_io).await;
        assert_eq!((kind, data_sid), (KIND_DATA, sid));
        write_frame(&mut host_io, KIND_DATA, sid, &data).await;
        let closed = HostEvent::StreamClosed { stream_id: sid };
        write_frame(&mut host_io, KIND_CONTROL, 0, &closed.encode()).await;
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("connect");
    let mut stream = client.open_service(AppService::Sam).await.expect("open");
    stream.write_all(b"ping").await.expect("write");
    let mut buf = vec![0u8; 16];
    let n = stream.read(&mut buf).await.expect("echo");
    assert_eq!(&buf[..n], b"ping");
    let n = stream.read(&mut buf).await.expect("eof read");
    assert_eq!(n, 0, "host close must produce EOF");
    host.await.expect("host");

    // Reset -> typed reset failure (with data sync first).
    let (client_io, mut host_io) = tokio::io::duplex(256 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io, vec!["sam".to_string()]).await;
        let (_, _, payload) = read_frame(&mut host_io).await;
        let sid = match AppRequest::decode(&payload).expect("open") {
            AppRequest::Open { stream_id, .. } => stream_id,
            other => panic!("open {other:?}"),
        };
        let ok = HostEvent::Reply {
            id: 2,
            ok: true,
            error: None,
        };
        write_frame(&mut host_io, KIND_CONTROL, 0, &ok.encode()).await;
        let (kind, data_sid, data) = read_frame(&mut host_io).await;
        assert_eq!((kind, data_sid), (KIND_DATA, sid));
        write_frame(&mut host_io, KIND_DATA, sid, &data).await;
        let reset = HostEvent::StreamReset {
            stream_id: sid,
            reason: Some("idle".to_string()),
        };
        write_frame(&mut host_io, KIND_CONTROL, 0, &reset.encode()).await;
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("connect");
    let mut stream = client.open_service(AppService::Sam).await.expect("open");
    stream.write_all(b"ping").await.expect("write");
    let mut buf = vec![0u8; 16];
    let n = stream.read(&mut buf).await.expect("echo");
    assert_eq!(&buf[..n], b"ping");
    let res = stream.read(&mut buf).await;
    assert!(res.is_err(), "host reset must fail reads");
    let kind = res.unwrap_err().kind();
    assert_eq!(kind, std::io::ErrorKind::ConnectionReset);
    host.await.expect("host");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sibling_isolation_close_one_keeps_other() {
    let (client_io, mut host_io) = tokio::io::duplex(512 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io, vec!["sam".to_string()]).await;
        // Two opens.
        let mut ids = Vec::new();
        for expected in [2u64, 3u64] {
            let (_, _, payload) = read_frame(&mut host_io).await;
            match AppRequest::decode(&payload).expect("open") {
                AppRequest::Open { id, stream_id, .. } => {
                    assert_eq!(id, expected);
                    ids.push(stream_id);
                }
                other => panic!("open {other:?}"),
            }
            let ok = HostEvent::Reply {
                id: expected,
                ok: true,
                error: None,
            };
            write_frame(&mut host_io, KIND_CONTROL, 0, &ok.encode()).await;
        }
        // Close the first; keep the second alive with an echo.
        let closed = HostEvent::StreamClosed { stream_id: ids[0] };
        write_frame(&mut host_io, KIND_CONTROL, 0, &closed.encode()).await;
        let (kind, sid, data) = read_frame(&mut host_io).await;
        assert_eq!(kind, KIND_DATA);
        assert_eq!(sid, ids[1]);
        write_frame(&mut host_io, KIND_DATA, ids[1], &data).await;
        // Expect close for the surviving stream after test drops it.
        let (_, _, payload) = read_frame(&mut host_io).await;
        assert_eq!(payload_close_id(&payload), ids[1]);
        let done = HostEvent::StreamClosed { stream_id: ids[1] };
        write_frame(&mut host_io, KIND_CONTROL, 0, &done.encode()).await;
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("connect");
    let mut first = client.open_service(AppService::Sam).await.expect("open1");
    let mut second = client.open_service(AppService::Sam).await.expect("open2");
    // First sees EOF after host close.
    let mut buf = vec![0u8; 16];
    let n = first.read(&mut buf).await.expect("first eof");
    assert_eq!(n, 0);
    // Second still echoes.
    second.write_all(b"sibling").await.expect("write");
    let mut buf = vec![0u8; 16];
    let n = second.read(&mut buf).await.expect("echo");
    assert_eq!(&buf[..n], b"sibling");
    drop(second);
    drop(first);
    host.await.expect("host");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stale_reply_id_is_fatal() {
    let (client_io, mut host_io) = tokio::io::duplex(256 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io, vec!["sam".to_string()]).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let stale = HostEvent::Reply {
            id: 999,
            ok: true,
            error: None,
        };
        write_frame(&mut host_io, KIND_CONTROL, 0, &stale.encode()).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("connect");
    host.await.expect("host");
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    assert!(client.is_closed(), "stale reply must terminate channel");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn data_before_open_is_fatal() {
    let (client_io, mut host_io) = tokio::io::duplex(256 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io, vec!["sam".to_string()]).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        write_frame(&mut host_io, KIND_DATA, 77, b"foreign").await;
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("connect");
    host.await.expect("host");
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    assert!(client.is_closed(), "data for unknown stream must be fatal");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fragmented_and_coalesced_traffic_survives() {
    let (client_io, mut host_io) = tokio::io::duplex(256 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        // Fragmented handshake read: client already sends handshake at once,
        // so fragment the host reply path instead: send reply + caps with
        // 1-byte writes to prove client read_exact handles fragments.
        let mut hb = [0u8; 8];
        host_io.read_exact(&mut hb).await.expect("handshake");
        let (_, _, _) = read_frame(&mut host_io).await;
        let reply = HostEvent::Reply {
            id: 1,
            ok: true,
            error: None,
        };
        let caps = HostEvent::Capabilities {
            granted: vec!["sam".to_string()],
        };
        let r1 = {
            let p = reply.encode();
            let h = encode_header(KIND_CONTROL, 0, p.len()).expect("h");
            [h.to_vec(), p].concat()
        };
        let r2 = {
            let p = caps.encode();
            let h = encode_header(KIND_CONTROL, 0, p.len()).expect("h");
            [h.to_vec(), p].concat()
        };
        // Coalesced: both frames in one write.
        let both = [r1, r2].concat();
        // Fragmented write: 1 byte at a time.
        for chunk in both.chunks(1) {
            host_io.write_all(chunk).await.expect("frag");
        }
        host_io.flush().await.expect("flush");
        // Open + coalesced data echo.
        let (_, _, payload) = read_frame(&mut host_io).await;
        let sid = match AppRequest::decode(&payload).expect("open") {
            AppRequest::Open { stream_id, .. } => stream_id,
            other => panic!("{other:?}"),
        };
        let ok = HostEvent::Reply {
            id: 2,
            ok: true,
            error: None,
        };
        write_frame(&mut host_io, KIND_CONTROL, 0, &ok.encode()).await;
        let (kind, sid2, data) = read_frame(&mut host_io).await;
        assert_eq!((kind, sid2), (KIND_DATA, sid));
        // Coalesce two data frames in one duplex write.
        let d1 = {
            let h = encode_header(KIND_DATA, sid, data.len()).expect("h");
            [h.to_vec(), data.clone()].concat()
        };
        let d2 = {
            let h = encode_header(KIND_DATA, sid, 3).expect("h");
            [h.to_vec(), b"xyz".to_vec()].concat()
        };
        host_io
            .write_all(&[d1, d2].concat())
            .await
            .expect("coalesce");
        host_io.flush().await.expect("flush");
        let (_, _, payload) = read_frame(&mut host_io).await;
        assert_eq!(payload_close_id(&payload), sid);
        let done = HostEvent::StreamClosed { stream_id: sid };
        write_frame(&mut host_io, KIND_CONTROL, 0, &done.encode()).await;
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("connect fragmented");
    let mut stream = client.open_service(AppService::Sam).await.expect("open");
    stream.write_all(b"abc").await.expect("write");
    let mut out = Vec::new();
    let mut buf = vec![0u8; 16];
    for _ in 0..2 {
        let n = stream.read(&mut buf).await.expect("read");
        out.extend_from_slice(&buf[..n]);
        if out.len() >= 6 {
            break;
        }
    }
    assert_eq!(out, b"abcxyz");
    drop(stream);
    host.await.expect("host");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn channel_eof_terminates_and_wakes_open() {
    let (client_io, mut host_io) = tokio::io::duplex(256 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io, vec!["sam".to_string()]).await;
        // EOF: drop without further frames.
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("connect");
    host.await.expect("host done");
    // Reader observes EOF and terminates the generation.
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
    assert!(client.is_closed());
    let err = match client.open_service(AppService::Sam).await {
        Ok(_) => panic!("expected closed"),
        Err(e) => e,
    };
    assert_eq!(err, ManagedError::ChannelClosed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn open_cancellation_releases_ids_without_reuse() {
    let (client_io, mut host_io) = tokio::io::duplex(256 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io, vec!["sam".to_string()]).await;
        // First open: read but never reply (forces client cancellation).
        let (_, _, payload) = read_frame(&mut host_io).await;
        let (first_id, first_sid) = match AppRequest::decode(&payload).expect("open1") {
            AppRequest::Open { id, stream_id, .. } => (id, stream_id),
            other => panic!("{other:?}"),
        };
        // Wait for cancellation best-effort close for the reserved id.
        let (_, _, payload) = tokio::time::timeout(
            tokio::time::Duration::from_secs(2),
            read_frame(&mut host_io),
        )
        .await
        .expect("close arrives");
        // The best-effort close must name the reserved stream, proving the
        // race was handled without leaking the id.
        assert_eq!(payload_close_id(&payload), first_sid);
        let _ = first_id;
        // Second open: reply normally.
        let (_, _, payload) = read_frame(&mut host_io).await;
        let (second_id, second_sid) = match AppRequest::decode(&payload).expect("open2") {
            AppRequest::Open { id, stream_id, .. } => (id, stream_id),
            other => panic!("{other:?}"),
        };
        assert!(second_id > first_id, "request ids must be monotonic");
        assert!(second_sid > first_sid, "stream ids must be monotonic");
        let ok = HostEvent::Reply {
            id: second_id,
            ok: true,
            error: None,
        };
        write_frame(&mut host_io, KIND_CONTROL, 0, &ok.encode()).await;
        let (_, _, payload) = read_frame(&mut host_io).await;
        assert_eq!(payload_close_id(&payload), second_sid);
        let done = HostEvent::StreamClosed {
            stream_id: second_sid,
        };
        write_frame(&mut host_io, KIND_CONTROL, 0, &done.encode()).await;
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("connect");
    // Cancel the first open via timeout.
    let cancelled = tokio::time::timeout(
        tokio::time::Duration::from_millis(150),
        client.open_service(AppService::Sam),
    )
    .await;
    assert!(cancelled.is_err(), "first open must time out");
    // Second open succeeds with fresh monotonic ids.
    let stream = tokio::time::timeout(
        tokio::time::Duration::from_secs(2),
        client.open_service(AppService::Sam),
    )
    .await
    .expect("second open scheduled")
    .expect("second open succeeds");
    assert!(stream.stream_id() > 1);
    drop(stream);
    host.await.expect("host");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn soak_open_close_returns_to_baseline() {
    let (client_io, mut host_io) = tokio::io::duplex(1024 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io, vec!["sam".to_string()]).await;
        for expected in 2u64..52u64 {
            let (_, _, payload) = read_frame(&mut host_io).await;
            match AppRequest::decode(&payload).expect("open") {
                AppRequest::Open { id, stream_id, .. } => {
                    assert_eq!(id, expected);
                    let ok = HostEvent::Reply {
                        id: expected,
                        ok: true,
                        error: None,
                    };
                    write_frame(&mut host_io, KIND_CONTROL, 0, &ok.encode()).await;
                    let (kind, sid, data) = read_frame(&mut host_io).await;
                    assert_eq!((kind, sid), (KIND_DATA, stream_id));
                    write_frame(&mut host_io, KIND_DATA, sid, &data).await;
                    let (_, _, payload) = read_frame(&mut host_io).await;
                    assert_eq!(payload_close_id(&payload), stream_id);
                    let done = HostEvent::StreamClosed { stream_id };
                    write_frame(&mut host_io, KIND_CONTROL, 0, &done.encode()).await;
                }
                other => panic!("{other:?}"),
            }
        }
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("connect");
    let mut last_id = 0;
    for _ in 0..50 {
        let mut stream = client.open_service(AppService::Sam).await.expect("open");
        assert!(stream.stream_id() > last_id);
        last_id = stream.stream_id();
        stream.write_all(b"soak").await.expect("write");
        let mut buf = vec![0u8; 8];
        let n = stream.read(&mut buf).await.expect("read");
        assert_eq!(&buf[..n], b"soak");
    }
    assert!(!client.is_closed());
    host.await.expect("host");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn error_display_carries_no_data_sentinel() {
    let sentinel = "SECRET-CREDENTIAL-PAYLOAD-BODY-12345";
    // Drive a fatal with the sentinel as opaque data payload.
    let (client_io, mut host_io) = tokio::io::duplex(256 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io, vec!["sam".to_string()]).await;
        write_frame(&mut host_io, KIND_DATA, 4321, sentinel.as_bytes()).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("connect");
    host.await.expect("host");
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    assert!(client.is_closed());
    let err = match client.open_service(AppService::Sam).await {
        Ok(_) => panic!("expected closed"),
        Err(e) => e,
    };
    assert!(!format!("{err}").contains(sentinel));
    assert!(!format!("{err:?}").contains(sentinel));
    for e in [
        ManagedError::Handshake,
        ManagedError::Frame,
        ManagedError::Control,
        ManagedError::Denied,
        ManagedError::ChannelClosed,
    ] {
        assert!(!format!("{e}").contains(sentinel));
        assert!(!format!("{e:?}").contains(sentinel));
    }
}
