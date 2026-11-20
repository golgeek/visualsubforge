use super::{Failure, releases::MAX_BYTES};
use reqwest::{Client, StatusCode};
use std::time::Duration;

pub(super) fn client(timeout: Duration, https_only: bool) -> Result<Client, Failure> {
    // reqwest's no-provider feature lets all targets use ring with platform roots.
    // Another TLS user may already have installed a process-wide provider.
    let _ = rustls::crypto::ring::default_provider().install_default();
    Client::builder()
        .https_only(https_only)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(timeout)
        .user_agent(concat!("VisualSubForge/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| Failure::Connection)
}

fn transport(error: reqwest::Error) -> Failure {
    if error.is_timeout() {
        Failure::Timeout
    } else {
        Failure::Connection
    }
}

pub(super) async fn read(client: Client, url: &str) -> Result<Vec<u8>, Failure> {
    let mut response = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(transport)?;
    match response.status() {
        StatusCode::TOO_MANY_REQUESTS | StatusCode::FORBIDDEN => return Err(Failure::RateLimit),
        StatusCode::OK => {}
        _ => return Err(Failure::Unavailable),
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_BYTES as u64)
    {
        return Err(Failure::TooLarge);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport)? {
        if chunk.len() > MAX_BYTES - body.len() {
            return Err(Failure::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    #[tokio::test]
    async fn interprets_bounded_http_responses_from_the_release_endpoint() {
        use crate::updates::releases::newest;
        use semver::Version;

        let version = Version::new(0, 1, 0);
        let newer = br#"[{"draft":false,"tag_name":"v0.2.0","prerelease":true}]"#;
        let older = br#"[{"draft":false,"tag_name":"v0.0.9"}]"#;
        for (body, expected) in [
            (newer.to_vec(), Ok(Some(Version::new(0, 2, 0)))),
            (older.to_vec(), Ok(None)),
            (b"unreadable".to_vec(), Err(Failure::Invalid)),
            (vec![b' '; MAX_BYTES + 1], Err(Failure::TooLarge)),
        ] {
            let server = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = server.local_addr().unwrap();
            let task = std::thread::spawn(move || {
                let (mut stream, _) = server.accept().unwrap();
                let mut request = Vec::new();
                std::io::BufRead::read_until(
                    &mut std::io::BufReader::new(&stream),
                    b'\n',
                    &mut request,
                )
                .unwrap();
                assert_eq!(request, b"GET /releases?per_page=20 HTTP/1.1\r\n");
                // Closing the connection delimits the body. Enforce the limit even
                // when the server does not announce its length.
                stream
                    .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n")
                    .unwrap();
                let _ = stream.write_all(&body);
            });
            let result = read(
                client(Duration::from_secs(3), false).unwrap(),
                &format!("http://{address}/releases?per_page=20"),
            )
            .await
            .and_then(|bytes| newest(&bytes, &version));
            assert_eq!(result, expected);
            task.join().unwrap();
        }
    }

    #[tokio::test]
    async fn bounds_transport_and_maps_plain_failures() {
        for (status, length, payload, stall, expected) in [
            ("200 OK", 2, "[]", false, Ok(vec![b'[', b']'])),
            ("403 Forbidden", 0, "", false, Err(Failure::RateLimit)),
            (
                "429 Too Many Requests",
                0,
                "",
                false,
                Err(Failure::RateLimit),
            ),
            ("200 OK", MAX_BYTES + 1, "", false, Err(Failure::TooLarge)),
            ("200 OK", 2, "[]", true, Err(Failure::Timeout)),
        ] {
            let server = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = server.local_addr().unwrap();
            let task = std::thread::spawn(move || {
                let (mut stream, _) = server.accept().unwrap();
                let mut request = [0; 4096];
                let _ = stream.read(&mut request);
                if stall {
                    std::thread::sleep(Duration::from_millis(250));
                }
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{payload}"
                );
            });
            assert_eq!(
                read(
                    client(Duration::from_millis(if stall { 100 } else { 2000 }), false).unwrap(),
                    &format!("http://{address}")
                )
                .await,
                expected
            );
            task.join().unwrap();
        }
    }
}
