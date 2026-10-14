use super::*;
use crate::network::fetch::tests::{Reply, Server};
use std::io::Write;
fn transport(origin: &str) -> HttpTransport {
    let mut t = HttpTransport::new(TransportOptions {
        dns_server: Some("100.100.100.100".parse().unwrap()),
        ..TransportOptions::default()
    })
    .unwrap();
    t.test_origins.insert(origin.into());
    t
}
#[tokio::test]
async fn codecs_preserve_bytes_and_bound_output_and_trailing_input() {
    use flate2::{
        Compression,
        write::{GzEncoder, ZlibEncoder},
    };
    use tokio::io::AsyncWriteExt;
    let data = b"|left_data=60,120,180|speed_data=120,2|".repeat(2048);
    let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
    gzip.write_all(&data).unwrap();
    let gzip = gzip.finish().unwrap();
    let mut zlib = ZlibEncoder::new(Vec::new(), Compression::default());
    zlib.write_all(&data).unwrap();
    let zlib = zlib.finish().unwrap();
    let mut br = async_compression::tokio::write::BrotliEncoder::new(Vec::new());
    br.write_all(&data).await.unwrap();
    br.shutdown().await.unwrap();
    let br = br.into_inner();
    for (name, body) in [("gzip", gzip.clone()), ("deflate", zlib), ("br", br)] {
        let original = data.clone();
        let encoded = body.clone();
        let n = name.to_string();
        let server = Server::new(move |_| {
            let mut r = Reply::body(&encoded);
            r.headers.push(("content-encoding".into(), n.clone()));
            r
        });
        let t = transport(&server.origin);
        let cancel = NetworkCancel::default();
        let deadline = Instant::now() + Duration::from_secs(5);
        let q = ReplayRequest {
            url: format!("{}/encoded", server.origin),
            context: RequestContext::default(),
            maximum_bytes: data.len(),
        };
        let p = t.prepare(&q.url, &cancel, deadline).await.unwrap();
        let mut counts = TransferCounts::default();
        let result = t
            .send(p, &q, 1024 * 1024, &cancel, deadline, &mut counts)
            .await
            .unwrap();
        assert_eq!(*result.body, original);
        assert_eq!(counts.wire_bytes, body.len() as u64);
        assert_eq!(counts.decoded_bytes, data.len() as u64);
        let mut counts = TransferCounts::default();
        let error = decode(body.clone(), name, 100, &cancel, deadline, &mut counts)
            .await
            .err()
            .unwrap();
        assert_eq!(error.code, "network-byte-limit");
        let mut trailing = body;
        trailing.extend_from_slice(b"garbage");
        assert!(
            decode(
                trailing,
                name,
                data.len() + 100,
                &cancel,
                deadline,
                &mut counts
            )
            .await
            .is_err(),
            "{name}"
        );
    }
    let cancel = NetworkCancel::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut counts = TransferCounts::default();
    let joined = [gzip.as_slice(), gzip.as_slice()].concat();
    assert_eq!(
        decode(
            joined,
            "gzip",
            data.len() * 2,
            &cancel,
            deadline,
            &mut counts
        )
        .await
        .unwrap(),
        data.repeat(2)
    );
    assert!(
        decode(
            gzip[..gzip.len() - 4].to_vec(),
            "gzip",
            data.len(),
            &cancel,
            deadline,
            &mut counts
        )
        .await
        .is_err()
    );
}
#[tokio::test]
async fn total_deadline_interrupts_trickling_body_and_partial_or_oversize_stops() {
    for kind in ["trickle", "partial", "wire", "codec", "drop"] {
        let k = kind.to_string();
        let server = Server::new(move |_| {
            let mut r = Reply::body(&[b'x'; 200]);
            match k.as_str() {
                "trickle" => r.trickle = true,
                "partial" => r.status = 206,
                "codec" => r.headers.push(("content-encoding".into(), "zstd".into())),
                "drop" => r.drop = true,
                _ => {}
            }
            r
        });
        let t = transport(&server.origin);
        let cancel = NetworkCancel::default();
        let deadline = Instant::now() + Duration::from_millis(150);
        let q = ReplayRequest {
            url: format!("{}/data", server.origin),
            context: RequestContext::default(),
            maximum_bytes: 1024,
        };
        let p = t.prepare(&q.url, &cancel, deadline).await.unwrap();
        let mut counts = TransferCounts::default();
        let error = t
            .send(
                p,
                &q,
                if kind == "wire" { 20 } else { 1024 },
                &cancel,
                deadline,
                &mut counts,
            )
            .await
            .err()
            .unwrap();
        assert_eq!(
            error.code,
            match kind {
                "trickle" => "request-timeout",
                "partial" => "partial-response",
                "wire" => "network-byte-limit",
                "codec" => "unsupported-encoding",
                _ => "http-error",
            },
            "{kind}"
        );
        assert_eq!(server.paths().len(), 1);
    }
}
#[tokio::test]
async fn async_dns_cancellation_and_mixed_answers_never_dial_an_http_host() {
    use hickory_resolver::{
        config::{ConnectionConfig, NameServerConfig},
        proto::{
            op::{Message, OpCode, ResponseCode},
            rr::{RData, Record, RecordType, rdata::A},
        },
    };
    for mixed in [false, true] {
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut connection = ConnectionConfig::udp();
        connection.port = socket.local_addr().unwrap().port();
        let config = ResolverConfig::from_name_servers(vec![NameServerConfig::new(
            "127.0.0.1".parse().unwrap(),
            true,
            vec![connection],
        )]);
        let mut builder =
            TokioResolver::builder_with_config(config, TokioRuntimeProvider::default());
        builder.options_mut().ip_strategy = LookupIpStrategy::Ipv4AndIpv6;
        builder.options_mut().attempts = 1;
        let mut t = HttpTransport::new(TransportOptions {
            dns_server: Some("100.100.100.100".parse().unwrap()),
            ..TransportOptions::default()
        })
        .unwrap();
        t.dns = builder.build().unwrap();
        let cancel = NetworkCancel::default();
        let trigger = cancel.clone();
        let (result, ()) = tokio::join!(
            t.prepare(
                "https://dns-fixture.example.org/song",
                &cancel,
                Instant::now() + Duration::from_secs(2)
            ),
            async {
                for _ in 0..if mixed { 2 } else { 1 } {
                    let mut b = [0u8; 4096];
                    let (n, peer) = socket.recv_from(&mut b).await.unwrap();
                    if !mixed {
                        trigger.cancel();
                        break;
                    }
                    let query = Message::from_vec(&b[..n]).unwrap();
                    let mut answer = Message::response(query.metadata.id, OpCode::Query);
                    answer.metadata.response_code = ResponseCode::NoError;
                    answer.metadata.recursion_desired = true;
                    answer.metadata.recursion_available = true;
                    answer.add_queries(query.queries.iter().cloned());
                    for q in &query.queries {
                        if q.query_type() == RecordType::A {
                            for ip in [
                                std::net::Ipv4Addr::new(8, 8, 8, 8),
                                std::net::Ipv4Addr::new(127, 0, 0, 1),
                            ] {
                                answer.add_answer(Record::from_rdata(
                                    q.name().clone(),
                                    1,
                                    RData::A(A(ip)),
                                ));
                            }
                        }
                    }
                    socket
                        .send_to(&answer.to_vec().unwrap(), peer)
                        .await
                        .unwrap();
                }
            }
        );
        assert_eq!(
            result.err().unwrap().code,
            if mixed {
                "private-network"
            } else {
                "request-cancelled"
            }
        );
    }
}

#[tokio::test]
async fn decoding_empty_members_still_observes_cancellation() {
    use flate2::{Compression, write::GzEncoder};
    let member = GzEncoder::new(Vec::new(), Compression::default())
        .finish()
        .unwrap();
    let input = member.repeat(100000);
    let cancel = NetworkCancel::default();
    let trigger = cancel.clone();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut counts = TransferCounts::default();
    let (result, ()) = tokio::join!(
        cancel.until(
            deadline,
            decode(input, "gzip", 100, &cancel, deadline, &mut counts)
        ),
        async {
            tokio::task::yield_now().await;
            trigger.cancel();
        }
    );
    assert_eq!(result.err().unwrap().code, "request-cancelled");
    assert_eq!(counts.decoded_bytes, 0);
}

#[tokio::test]
async fn explicit_ca_bundle_requires_exact_bounded_nonempty_roots() {
    let path = std::env::temp_dir().join(format!(
        "ddi-root-bundle-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, b"not a certificate").unwrap();
    for hash in ["0".repeat(64), sha256(b"not a certificate")] {
        let options = TransportOptions {
            dns_server: Some("100.100.100.100".parse().unwrap()),
            tls_root_bundle: Some(RootBundle {
                path: path.clone(),
                sha256: hash,
            }),
            ..Default::default()
        };
        assert_eq!(
            HttpTransport::new(options).err().unwrap().code,
            "tls-root-bundle"
        );
    }
    std::fs::remove_file(path).unwrap();
}
