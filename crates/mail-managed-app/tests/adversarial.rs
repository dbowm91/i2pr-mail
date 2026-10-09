//! Adversarial channel-level tests: ceilings, malformed framing, and
//! hostile control payloads. All use injected duplex I/O.

use i2pr_mail_managed_app::{
    AppService, ManagedAppClient, ManagedLaunchIdentity,
    client::ManagedError,
    control::{AppRequest, HostEvent},
    frame::{FRAME_HEADER_LEN, KIND_CONTROL, KIND_DATA, MAX_FRAME_PAYLOAD_LEN},
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
    let header =
        i2pr_mail_managed_app::frame::encode_header(kind, stream_id, payload.len()).expect("frame");
    io.write_all(&header).await.expect("h");
    io.write_all(payload).await.expect("p");
    io.flush().await.expect("f");
}

async fn read_frame<T>(io: &mut T) -> (u8, u32, Vec<u8>)
where
    T: AsyncReadExt + Unpin,
{
    let mut hb = [0u8; FRAME_HEADER_LEN];
    io.read_exact(&mut hb).await.expect("rh");
    let h = i2pr_mail_managed_app::frame::decode_header(&hb).expect("dh");
    let mut payload = vec![0u8; h.payload_len];
    if !payload.is_empty() {
        io.read_exact(&mut payload).await.expect("rp");
    }
    (h.kind, h.stream_id, payload)
}

async fn host_establish<T>(io: &mut T)
where
    T: AsyncReadExt + AsyncWriteExt + Unpin,
{
    let mut hb = [0u8; 8];
    io.read_exact(&mut hb).await.expect("hs");
    decode_handshake(&hb).expect("hd");
    let (_, _, payload) = read_frame(io).await;
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
    let caps = HostEvent::Capabilities {
        granted: vec!["sam".to_string()],
    };
    write_frame(io, KIND_CONTROL, 0, &caps.encode()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn request_ledger_saturates_at_64() {
    let (client_io, mut host_io) = tokio::io::duplex(1024 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io).await;
        // Read opens forever without replying; break on EOF.
        loop {
            let mut hb = [0u8; FRAME_HEADER_LEN];
            if host_io.read_exact(&mut hb).await.is_err() {
                break;
            }
            let h = i2pr_mail_managed_app::frame::decode_header(&hb).expect("h");
            let mut p = vec![0u8; h.payload_len];
            if !p.is_empty() && host_io.read_exact(&mut p).await.is_err() {
                break;
            }
        }
    });
    let client = std::sync::Arc::new(
        ManagedAppClient::connect(client_io, &identity)
            .await
            .expect("connect"),
    );
    // 64 concurrent opens, none replied.
    let mut handles = Vec::new();
    for _ in 0..64 {
        let c = std::sync::Arc::clone(&client);
        handles.push(tokio::spawn(async move {
            c.open_service(AppService::Sam).await
        }));
    }
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    let err = match client.open_service(AppService::Sam).await {
        Ok(_) => panic!("expected saturated"),
        Err(e) => e,
    };
    assert_eq!(err, ManagedError::RequestSaturated);
    client.shutdown();
    for h in handles {
        h.abort();
    }
    host.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stream_ledger_saturates_at_128() {
    let (client_io, mut host_io) = tokio::io::duplex(4 * 1024 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io).await;
        loop {
            let res = tokio::time::timeout(
                tokio::time::Duration::from_millis(500),
                read_frame(&mut host_io),
            )
            .await;
            let (_, _, payload) = match res {
                Ok(f) => f,
                Err(_) => break,
            };
            match AppRequest::decode(&payload).expect("req") {
                AppRequest::Open { id, stream_id, .. } => {
                    let ok = HostEvent::Reply {
                        id,
                        ok: true,
                        error: None,
                    };
                    write_frame(&mut host_io, KIND_CONTROL, 0, &ok.encode()).await;
                    let _ = stream_id;
                }
                AppRequest::Close { .. } | AppRequest::Reset { .. } => {}
                other => panic!("unexpected {other:?}"),
            }
        }
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("connect");
    let mut streams = Vec::new();
    for _ in 0..128 {
        streams.push(client.open_service(AppService::Sam).await.expect("open"));
    }
    let err = match client.open_service(AppService::Sam).await {
        Ok(_) => panic!("expected saturated"),
        Err(e) => e,
    };
    assert_eq!(err, ManagedError::StreamSaturated);
    drop(streams);
    client.shutdown();
    host.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn oversize_frame_header_is_fatal() {
    let (client_io, mut host_io) = tokio::io::duplex(256 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        // Craft header declaring max+1 payload without sending it.
        let len = (MAX_FRAME_PAYLOAD_LEN as u32 + 1).to_be_bytes();
        let mut header = [0u8; FRAME_HEADER_LEN];
        header[0] = 1;
        header[1] = KIND_DATA;
        header[4..8].copy_from_slice(&9u32.to_be_bytes());
        header[8..12].copy_from_slice(&len);
        host_io.write_all(&header).await.expect("oversize");
        host_io.flush().await.expect("f");
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("connect");
    host.await.expect("host");
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
    assert!(client.is_closed(), "oversize header must be fatal");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn truncated_payload_terminates_channel() {
    let (client_io, mut host_io) = tokio::io::duplex(256 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io).await;
        // Declare 100 bytes, send 10, then drop (EOF).
        let header = i2pr_mail_managed_app::frame::encode_header(KIND_DATA, 5, 100).expect("h");
        host_io.write_all(&header).await.expect("h");
        host_io.write_all(&[0u8; 10]).await.expect("partial");
        host_io.flush().await.expect("f");
        // Drop without the remaining 90 bytes.
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("connect");
    host.await.expect("host");
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
    assert!(client.is_closed(), "truncated payload must terminate");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wrong_direction_control_is_fatal() {
    let (client_io, mut host_io) = tokio::io::duplex(256 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        // Host sends an application-direction message.
        let fake_hello = AppRequest::Hello {
            id: 5,
            app_id: "mail/test-client".to_string(),
            instance: "7".to_string(),
        };
        write_frame(&mut host_io, KIND_CONTROL, 0, &fake_hello.encode()).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("connect");
    host.await.expect("host");
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
    assert!(client.is_closed(), "wrong-direction must be fatal");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn duplicate_control_keys_fatal() {
    let (client_io, mut host_io) = tokio::io::duplex(256 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let raw = br#"{"type":"reply","id":7,"id":8,"ok":true}"#;
        write_frame(&mut host_io, KIND_CONTROL, 0, raw).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("connect");
    host.await.expect("host");
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
    assert!(client.is_closed(), "duplicate keys must be fatal");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unknown_control_field_is_fatal() {
    let (client_io, mut host_io) = tokio::io::duplex(256 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let raw = br#"{"type":"reply","id":7,"ok":true,"extra":1}"#;
        write_frame(&mut host_io, KIND_CONTROL, 0, raw).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("connect");
    host.await.expect("host");
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
    assert!(client.is_closed(), "unknown field must be fatal");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unknown_control_type_is_fatal() {
    let (client_io, mut host_io) = tokio::io::duplex(256 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let raw = br#"{"type":"permission_request","id":1}"#;
        write_frame(&mut host_io, KIND_CONTROL, 0, raw).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("connect");
    host.await.expect("host");
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
    assert!(client.is_closed(), "unknown type must be fatal");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn inbound_backpressure_delivers_all_without_drop() {
    let (client_io, mut host_io) = tokio::io::duplex(1024 * 1024);
    let identity = test_identity();
    let host = tokio::spawn(async move {
        host_establish(&mut host_io).await;
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
        // 40 small frames back-to-back (beyond the 32-chunk queue).
        for i in 0u8..40 {
            write_frame(&mut host_io, KIND_DATA, sid, &[i]).await;
        }
        let (_, _, payload) = read_frame(&mut host_io).await;
        assert_eq!(
            match AppRequest::decode(&payload).expect("close") {
                AppRequest::Close { stream_id } => stream_id,
                other => panic!("{other:?}"),
            },
            sid
        );
        let done = HostEvent::StreamClosed { stream_id: sid };
        write_frame(&mut host_io, KIND_CONTROL, 0, &done.encode()).await;
    });
    let client = ManagedAppClient::connect(client_io, &identity)
        .await
        .expect("connect");
    let mut stream = client.open_service(AppService::Sam).await.expect("open");
    let mut got = Vec::new();
    let mut buf = vec![0u8; 8];
    while got.len() < 40 {
        let n = tokio::time::timeout(tokio::time::Duration::from_secs(2), stream.read(&mut buf))
            .await
            .expect("read scheduled")
            .expect("read");
        if n == 0 {
            break;
        }
        got.extend_from_slice(&buf[..n]);
    }
    assert_eq!(got.len(), 40);
    for (i, b) in got.iter().enumerate() {
        assert_eq!(*b, i as u8);
    }
    drop(stream);
    host.await.expect("host");
}
