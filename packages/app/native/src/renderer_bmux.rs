//! Application-owned, connection-scoped artwork transport for the terminal renderer.

use std::{io, sync::Arc, time::Duration};

use hyperchad::renderer_bmux::renderer::{
    BmuxConfig, Error, HostCallbacks, HostRequest, HostResponse,
};
use moosicbox_app_models::ConnectionStatus;
use reqwest::{Client, Url};

const MAX_BYTES: usize = 8 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(15);

fn denied(message: &'static str) -> Error {
    io::Error::other(message).into()
}

/// Builds terminal policy using the application's current connection for each artwork request.
///
/// Only configured-origin album/artist artwork is authorized. No redirects, arbitrary HTTP
/// effects, filesystem access, external links, or remote cookies are authorized here.
#[must_use]
pub fn config() -> BmuxConfig {
    BmuxConfig {
        host: HostCallbacks {
            request: Some(Arc::new(|operation| {
                Box::pin(async move {
                    match operation {
                        HostRequest::Resource { source, max_bytes } => {
                            let connection = Connection::current().await?;
                            let url = connection.authorize(&source)?;
                            let bytes = fetch(&connection, url, max_bytes).await?;
                            Ok(HostResponse::Bytes(bytes))
                        }
                        // These notifications contain no origin; never import remote cookies.
                        HostRequest::Cookies(_) => Ok(HostResponse::Complete),
                        _ => Err(denied("operation not authorized by artwork policy")),
                    }
                })
            })),
        },
        resource_context: Some(Arc::new(|| crate::STATE.resource_context_epoch())),
        ..BmuxConfig::default()
    }
}

struct Connection {
    base: Url,
    profile: String,
    token: Option<String>,
    client_id: Option<String>,
    signature: Option<String>,
}

impl Connection {
    async fn current() -> Result<Self, Error> {
        Self::from_state(&crate::STATE).await
    }

    async fn from_state(state: &moosicbox_app_state::AppState) -> Result<Self, Error> {
        // HTTP artwork does not depend on WebSocket readiness. The state snapshot owns
        // publication synchronization; never retain a state lock while awaiting it.
        if matches!(
            *state.connection_status.read().await,
            ConnectionStatus::Unconfigured | ConnectionStatus::ShuttingDown
        ) {
            return Err(denied("artwork requires a configured connection"));
        }
        let config = state
            .connection_resource_snapshot()
            .await
            .ok_or_else(|| denied("artwork requires a configured connection"))?;
        Ok(Self {
            base: Url::parse(config.api_url()).map_err(|_| denied("invalid connection URL"))?,
            profile: config.profile().to_string(),
            token: config.api_token().map(str::to_owned),
            client_id: config.client_id().map(str::to_owned),
            signature: config.signature_token().map(str::to_owned),
        })
    }

    fn authorize(&self, source: &str) -> Result<Url, Error> {
        let base = &self.base;
        if !matches!(base.scheme(), "http" | "https")
            || base.host_str().is_none()
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
        {
            return Err(denied("invalid artwork connection URL"));
        }
        // Reject ambiguous encodings before URL parsing can normalize traversal away.
        let raw_path = source.split(['?', '#']).next().unwrap_or(source);
        if raw_path.contains('%')
            || raw_path.contains('\\')
            || raw_path.split('/').any(|part| matches!(part, "." | ".."))
            || source.chars().any(char::is_control)
        {
            return Err(denied("ambiguous artwork path"));
        }
        // UI artwork URLs are absolute; resolving arbitrary relatives would broaden policy.
        let mut url = Url::parse(source).map_err(|_| denied("artwork URL must be absolute"))?;
        if url.origin() != base.origin()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(denied("artwork origin not authorized"));
        }
        let prefix = format!("{}/files/", base.path().trim_end_matches('/'));
        let path = url
            .path()
            .strip_prefix(&prefix)
            .ok_or_else(|| denied("artwork path not authorized"))?;
        let parts: Vec<_> = path.split('/').collect();
        if parts.len() != 3
            || !matches!(parts[0], "albums" | "artists")
            || parts[1].is_empty()
            || !parts[1]
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
            || !valid_size(parts[2])
        {
            return Err(denied("artwork route not authorized"));
        }
        let mut source_value = None;
        let mut profile_seen = false;
        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "source" if source_value.is_none() && !value.is_empty() && value.len() <= 128 => {
                    source_value = Some(value.into_owned());
                }
                // UI currently emits master. Replace it with the active profile below.
                "moosicboxProfile" if !profile_seen => profile_seen = true,
                _ => return Err(denied("artwork query not authorized")),
            }
        }
        url.set_query(None);
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("moosicboxProfile", &self.profile);
            if let Some(source) = source_value {
                query.append_pair("source", &source);
            }
            if let Some(client_id) = self.client_id.as_deref().filter(|id| !id.is_empty()) {
                query.append_pair("clientId", client_id);
                if let Some(signature) = self.signature.as_deref().filter(|value| !value.is_empty())
                {
                    query.append_pair("signature", signature);
                }
            }
        }
        Ok(url)
    }
}

fn valid_size(value: &str) -> bool {
    value.split_once('x').is_some_and(|(width, height)| {
        <[&str; 2]>::from((width, height)).into_iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|c| c.is_ascii_digit())
                && part
                    .parse::<u16>()
                    .is_ok_and(|size| (1..=4096).contains(&size))
        })
    })
}

async fn fetch(connection: &Connection, url: Url, max_bytes: usize) -> Result<Vec<u8>, Error> {
    let limit = max_bytes.min(MAX_BYTES);
    if limit == 0 {
        return Err(denied("empty artwork byte budget"));
    }
    // Deliberately support HTTP when the application has explicitly configured it, including
    // local servers. Never follow even same-origin redirects; a default client could leak auth.
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .connect_timeout(Duration::from_secs(5))
        .build()
        .map_err(|_| denied("artwork client initialization failed"))?;
    switchy_async::time::timeout(TIMEOUT, async {
        let mut request = client
            .get(url)
            .header("moosicbox-profile", &connection.profile);
        if let Some(token) = connection
            .token
            .as_deref()
            .filter(|token| !token.is_empty())
        {
            request = request.bearer_auth(token);
        }
        // Do not surface reqwest errors containing credential-bearing URLs to renderer logs.
        let mut response = request
            .send()
            .await
            .map_err(|_| denied("artwork request failed"))?;
        if !response.status().is_success() {
            return Err(denied("artwork response was not successful"));
        }
        if response
            .content_length()
            .is_some_and(|size| size > limit as u64)
        {
            return Err(denied("artwork response exceeds byte budget"));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| denied("artwork body failed"))?
        {
            if chunk.len() > limit.saturating_sub(bytes.len()) {
                return Err(denied("artwork response exceeds byte budget"));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    })
    .await
    .map_err(|_| denied("artwork request timed out"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read as _, Write as _};

    fn connection(base: &str) -> Connection {
        Connection {
            base: Url::parse(base).unwrap(),
            profile: "private profile".into(),
            token: Some("isolated-test-token".into()),
            client_id: Some("client&isolated=1".into()),
            signature: Some("isolated-signature&value".into()),
        }
    }

    #[test]
    fn exact_origin_and_artwork_route_policy() {
        let connection = connection("https://music.example/api");
        for suffix in ["albums/123/80x80", "artists/provider-id/1x4096"] {
            let url = connection.authorize(&format!(
                "https://music.example/api/files/{suffix}?source=LIBRARY&moosicboxProfile=master"
            )).unwrap();
            let query: std::collections::BTreeMap<_, _> = url.query_pairs().into_owned().collect();
            assert_eq!(query["moosicboxProfile"], "private profile");
            assert_eq!(query["clientId"], "client&isolated=1");
            assert_eq!(query["source"], "LIBRARY");
            assert_eq!(query["signature"], "isolated-signature&value");
            assert!(!url.as_str().contains("isolated-test-token"));
        }
        for url in [
            "https://evil.example/api/files/albums/1/80x80",
            "http://music.example/api/files/albums/1/80x80",
            "https://music.example:444/api/files/albums/1/80x80",
            "https://user@music.example/api/files/albums/1/80x80",
            "https://music.example/files/albums/1/80x80",
            "https://music.example/api/files/tracks/1/80x80",
            "https://music.example/api/files/albums/1/80x80/extra",
            "https://music.example/api/files/albums/1/0x80",
            "https://music.example/api/files/albums/1/80x4097",
            "https://music.example/api/files/albums/%2e%2e/80x80",
            "https://music.example/api/files/albums/a%2fb/80x80",
            "https://music.example/api/files/albums/../albums/1/80x80",
            "https://music.example/api/files/albums/1/80x80?clientId=stolen",
            "https://music.example/api/files/albums/1/80x80?signature=stolen",
            "https://music.example/api/files/albums/1/80x80?source=A&source=B",
            "https://music.example/api/files/albums/1/80x80#fragment",
            "file:///api/files/albums/1/80x80",
            "/api/files/albums/1/80x80",
        ] {
            assert!(connection.authorize(url).is_err(), "accepted {url}");
        }
    }

    fn mock_server(response: &'static [u8]) -> (String, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let count = socket.read(&mut buffer).unwrap();
                assert!(count > 0 && request.len() < 16 * 1024);
                request.extend_from_slice(&buffer[..count]);
            }
            socket.write_all(response).unwrap();
            String::from_utf8(request).unwrap()
        });
        (base, task)
    }

    #[test]
    fn local_http_auth_and_streaming_limits() {
        let runtime = switchy::unsync::runtime::Builder::new().build().unwrap();
        for (response, budget, succeeds) in [
            (&b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc"[..], 3, true),
            (&b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nabcd"[..], 3, false),
            (&b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n2\r\nab\r\n2\r\ncd\r\n0\r\n\r\n"[..], 3, false),
            (&b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"[..], 3, false),
        ] {
            let (base, server) = mock_server(response);
            let connection = connection(&base);
            let url = connection.authorize(&format!("{base}/files/albums/1/80x80?source=LIBRARY")).unwrap();
            let result = runtime.block_on(fetch(&connection, url, budget));
            assert_eq!(result.is_ok(), succeeds);
            if succeeds {
                assert_eq!(result.unwrap(), b"abc");
            }
            let request = server.join().unwrap().to_ascii_lowercase();
            assert!(request.contains("authorization: bearer isolated-test-token\r\n"));
            assert!(request.contains("moosicbox-profile: private profile\r\n"));
            assert!(request.contains("clientid=client%26isolated%3d1"));
            assert!(request.contains("signature=isolated-signature%26value"));
            assert!(!request.contains("cookie:"));
        }
    }

    #[test]
    fn redirects_never_receive_credentials() {
        let runtime = switchy::unsync::runtime::Builder::new().build().unwrap();
        let target = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        target.set_nonblocking(true).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let target_addr = target.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut buffer = [0; 4096];
            let _ = socket.read(&mut buffer).unwrap();
            write!(socket, "HTTP/1.1 302 Found\r\nLocation: http://{target_addr}/steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let connection = connection(&base);
        let url = connection
            .authorize(&format!("{base}/files/artists/1/80x80"))
            .unwrap();
        assert!(runtime.block_on(fetch(&connection, url, 100)).is_err());
        server.join().unwrap();
        assert_eq!(
            target.accept().unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn state_is_read_per_request_without_user_configuration() {
        let runtime = switchy::unsync::runtime::Builder::new().build().unwrap();
        runtime.block_on(async {
            let state = moosicbox_app_state::AppState::new();
            assert!(Connection::from_state(&state).await.is_err());
            for (endpoint, profile, token) in [
                ("http://127.0.0.1:1234", "first", "token-a"),
                ("https://second.invalid/api", "second", "token-b"),
            ] {
                *state.connection_config.write().await = Some(
                    moosicbox_app_state::ConnectionConfig::new(endpoint, profile, "test").unwrap(),
                );
                *state.connection_id.write().await = Some("test".into());
                *state.api_url.write().await = Some(endpoint.into());
                *state.profile.write().await = Some(profile.into());
                *state.api_token.write().await = Some(token.into());
                *state.connection_status.write().await = ConnectionStatus::Ready;
                let connection = Connection::from_state(&state).await.unwrap();
                assert_eq!(connection.base.as_str().trim_end_matches('/'), endpoint);
                assert_eq!(connection.profile, profile);
                assert_eq!(connection.token.as_deref(), Some(token));
            }
            for status in [
                ConnectionStatus::Connecting,
                ConnectionStatus::Reconnecting { attempt: 1 },
                ConnectionStatus::AuthenticationRequired,
            ] {
                *state.connection_status.write().await = status;
                assert!(Connection::from_state(&state).await.is_ok());
            }
            // Same endpoint changes must not retain stale profile or refreshed credentials.
            *state.profile.write().await = Some("changed-profile".into());
            *state.api_token.write().await = Some("refreshed-token".into());
            *state.client_id.write().await = Some("changed-client".into());
            *state.signature_token.write().await = Some("refreshed-signature".into());
            let refreshed = Connection::from_state(&state).await.unwrap();
            assert_eq!(refreshed.profile, "changed-profile");
            assert_eq!(refreshed.token.as_deref(), Some("refreshed-token"));
            assert_eq!(refreshed.client_id.as_deref(), Some("changed-client"));
            assert_eq!(refreshed.signature.as_deref(), Some("refreshed-signature"));
            *state.api_url.write().await = Some("https://mismatched.invalid".into());
            assert!(Connection::from_state(&state).await.is_err());
            *state.connection_status.write().await = ConnectionStatus::Unconfigured;
            assert!(Connection::from_state(&state).await.is_err());
        });
    }
}
