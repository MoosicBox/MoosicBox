//! Terminal-app local transport. This adapter never opens files, launches a browser, or
//! performs network IO. One `attach` call creates one isolated, in-memory local-origin jar.
//!
//! Parent wiring: call `attach(config, router.clone())` exactly once when building a renderer.
//! With `assets`, call `attach_assets` before `attach` for exact in-memory registrations.
//! File/directory registrations require the parent's explicit bounded filesystem host policy.
//! No asset wrapper accesses the filesystem, including for canonicalization or metadata.
//! Cookie notifications are forwarded to the user's host policy, not consumed as permission
//! to persist remote cookies. Only cookies returned by this adapter's local routes enter its jar.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use hyperchad_renderer::{Content, ResponseCookie, SameSite};
use hyperchad_renderer_bmux::{
    actions::{FormPart, MultipartSegment, RequestBody, RequestEffect},
    renderer::{BmuxConfig, Error, HostCallbacks, HostRequest, HostResponse},
};
use hyperchad_router::{RequestInfo, RouteRequest, Router};
use switchy::http::models::Method;

const MAX_COOKIES: usize = 256;
const MAX_COOKIE_BYTES: usize = 4096;
const URI_HEADER: &str = "x-hyperchad-request-target";

/// Install app-local route transport, retaining the explicit user callback for every operation
/// outside that scope. Requests are executed once, never via `navigate_send`'s broadcast channel.
/// The runtime retains source/target/strategy/correlation from the original `RequestEffect`.
#[must_use]
pub(super) fn attach(mut config: BmuxConfig, router: Router) -> BmuxConfig {
    let previous = config.host.request.take();
    let jar = Arc::new(Mutex::new(CookieJar::default()));
    let max_bytes = config.max_text_bytes;
    config.host = HostCallbacks {
        request: Some(Arc::new(move |operation| {
            let previous = previous.clone();
            let router = router.clone();
            let jar = jar.clone();
            Box::pin(async move {
                if let HostRequest::Http(request) = &operation {
                    // A local callback must not resolve host-authorized file capabilities. Delegate
                    // the entire request intact, never replay it locally after host execution.
                    let files = matches!(&request.body, RequestBody::Multipart(parts) if parts.iter().any(|p| matches!(p,FormPart::File { .. })));
                    if !files && let Some(uri) = local_uri(&request.url, &request.base_url)? {
                        let path = uri.split('?').next().unwrap_or(&uri);
                        if let Some(handler) = router.get_route_func(path) {
                            let route_request = prepare(request, &uri, &jar, max_bytes)?;
                            let result = handler(route_request)
                                .await
                                .map_err(|error| failure(error.to_string()))?;
                            return response(result, &uri, &jar, max_bytes);
                        }
                    }
                }
                // Local responses were already applied to this private jar in `response`.
                // A bare cookie notification has no origin, so it must never import remote
                // cookies into the local origin. Forward it unchanged to explicit host policy;
                // absent policy, acknowledge the in-memory local-only handling.
                if matches!(operation, HostRequest::Cookies(_)) && previous.is_none() {
                    return Ok(HostResponse::Complete);
                }
                if let Some(callback) = previous {
                    callback(operation).await
                } else {
                    Err(failure(
                        "operation requires an explicit application host policy",
                    ))
                }
            })
        })),
    };
    config
}

/// Serve exact registered in-memory assets, leaving filesystem registrations and unknown
/// resources to the explicit host policy. Call before `attach`; no registration causes IO.
/// Request and configured renderer byte limits are both enforced before copying bytes.
#[cfg(feature = "assets")]
#[must_use]
pub(super) fn attach_assets(
    mut config: BmuxConfig,
    assets: Vec<hyperchad_renderer::assets::StaticAssetRoute>,
) -> BmuxConfig {
    use hyperchad_renderer::assets::AssetPathTarget;
    let previous = config.host.request.take();
    let max_config_bytes = config.max_text_bytes;
    let assets = Arc::new(assets);
    config.host.request = Some(Arc::new(move |operation| {
        let previous = previous.clone();
        let assets = assets.clone();
        Box::pin(async move {
            if let HostRequest::Resource { source, max_bytes } = &operation
                && let Some(path) = asset_path(source)?
            {
                // First exact registered memory route wins, matching registration order.
                for asset in assets.iter() {
                    if let AssetPathTarget::FileContents(bytes) = &asset.target
                        && asset_path(&asset.route)?.as_deref() == Some(path.as_str())
                    {
                        if bytes.len() > (*max_bytes).min(max_config_bytes) {
                            return Err(failure("registered memory asset exceeds byte limit"));
                        }
                        return Ok(HostResponse::Bytes(bytes.to_vec()));
                    }
                }
            }
            if let Some(callback) = previous {
                callback(operation).await
            } else {
                Err(failure(
                    "resource requires explicit host callback; file/directory registrations are not read by the local transport",
                ))
            }
        })
    }));
    config
}

/// Strict single-pass URL path decoding. Never normalize traversal into an allowed asset.
/// External URLs go intact to explicit host policy. Encoded separators, double encodings,
/// backslashes, controls and dot segments fail closed even when a fallback exists.
#[cfg(feature = "assets")]
fn asset_path(source: &str) -> Result<Option<String>, Error> {
    if source.len() > 1024 * 1024
        || source.chars().any(char::is_control)
        || source.contains('\\')
        || source.trim() != source
    {
        return Err(failure("invalid asset URL"));
    }
    if source.starts_with("//")
        || source
            .split(['/', '?', '#'])
            .next()
            .is_some_and(|part| part.contains(':'))
    {
        return Ok(None);
    }
    let path = source.split(['?', '#']).next().unwrap_or(source);
    if !path.starts_with('/') {
        return Ok(None);
    }
    let mut decoded = Vec::with_capacity(path.len());
    let mut bytes = path.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let hi = bytes.next().and_then(|b| char::from(b).to_digit(16));
            let lo = bytes.next().and_then(|b| char::from(b).to_digit(16));
            let (Some(hi), Some(lo)) = (hi, lo) else {
                return Err(failure("malformed asset URL escape"));
            };
            let value = u8::try_from(hi * 16 + lo).map_err(|_| failure("asset URL escape"))?;
            if matches!(value, b'/' | b'\\' | b'%' | b'?' | b'#') {
                return Err(failure("encoded asset separator or double encoding"));
            }
            decoded.push(value);
        } else {
            decoded.push(byte);
        }
    }
    let decoded = String::from_utf8(decoded).map_err(|_| failure("asset path is not UTF-8"))?;
    if decoded.chars().any(char::is_control)
        || decoded
            .split('/')
            .any(|segment| matches!(segment, "." | ".."))
    {
        return Err(failure("asset traversal or control character"));
    }
    Ok(Some(decoded))
}

fn failure(message: impl Into<String>) -> Error {
    Box::new(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        message.into(),
    ))
}

/// Resolve only local app paths. Absolute URLs, protocol-relative references and operations
/// relative to a remote base are delegated to user policy, even if the hostname is localhost.
fn local_uri(url: &str, base: &str) -> Result<Option<String>, Error> {
    if url.len() > 1024 * 1024
        || base.len() > 1024 * 1024
        || url.chars().any(char::is_control)
        || base.chars().any(char::is_control)
        || url.contains('\\')
        || base.contains('\\')
        || url.trim() != url
    {
        return Err(failure("invalid route URL"));
    }
    let external = |text: &str| {
        text.starts_with("//")
            || text
                .split(['/', '?', '#'])
                .next()
                .is_some_and(|part| part.contains(':'))
    };
    if external(url) || external(base) {
        return Ok(None);
    }
    let url = url.split('#').next().unwrap_or(url);
    let base = base.split('#').next().unwrap_or(base);
    let combined = if url.starts_with('/') {
        url.to_owned()
    } else if url.is_empty() {
        if base.is_empty() {
            "/".into()
        } else {
            base.to_owned()
        }
    } else if url.starts_with('?') {
        format!(
            "{}{url}",
            base.split('?')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("/")
        )
    } else {
        let base_path = base.split('?').next().unwrap_or("/");
        let prefix = base_path.rsplit_once('/').map_or("/", |(prefix, _)| prefix);
        format!("{prefix}/{url}")
    };
    let (path, query) = combined
        .split_once('?')
        .map_or((combined.as_str(), None), |(p, q)| (p, Some(q)));
    let mut segments = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            segment => segments.push(segment),
        }
    }
    let mut normalized = format!("/{}", segments.join("/"));
    if path.ends_with('/') && !normalized.ends_with('/') {
        normalized.push('/');
    }
    if let Some(query) = query {
        normalized.push('?');
        normalized.push_str(query);
    }
    Ok(Some(normalized))
}

fn prepare(
    request: &RequestEffect,
    uri: &str,
    jar: &Mutex<CookieJar>,
    max_bytes: usize,
) -> Result<RouteRequest, Error> {
    let mut route = prepare_metadata(request, uri, jar, max_bytes)?;
    let body = match &request.body {
        RequestBody::Empty => None,
        RequestBody::UrlEncoded(body) => {
            if body.len() > max_bytes {
                return Err(failure("local request body exceeds limit"));
            }
            route
                .headers
                .entry("content-type".into())
                .or_insert_with(|| "application/x-www-form-urlencoded;charset=UTF-8".into());
            Some(body.as_bytes().to_vec())
        }
        RequestBody::Multipart(parts) => {
            if parts.len() > 4096 || parts.iter().any(|p| matches!(p, FormPart::File { .. })) {
                return Err(failure("file multipart requires explicit host transport"));
            }
            let mut metadata_bytes = 0_usize;
            for part in parts {
                if let FormPart::Text { name, value } = part {
                    metadata_bytes = metadata_bytes
                        .saturating_add(name.len())
                        .saturating_add(value.len());
                }
            }
            if metadata_bytes > max_bytes {
                return Err(failure("local multipart body exceeds limit"));
            }
            let mut sequence = 0_u64;
            let boundary = loop {
                let candidate = format!("hyperchad-bmux-local-{sequence:016x}");
                if !parts.iter().any(|part| match part {
                    FormPart::Text { name, value } => {
                        name.contains(&candidate) || value.contains(&candidate)
                    }
                    FormPart::File { .. } => true,
                }) {
                    break candidate;
                }
                sequence = sequence
                    .checked_add(1)
                    .ok_or_else(|| failure("multipart boundary exhausted"))?;
                if sequence > 4096 {
                    return Err(failure("multipart boundary budget exceeded"));
                }
            };
            let segments = request
                .body
                .multipart_segments(&boundary, max_bytes as u64)
                .map_err(|e| failure(e.to_string()))?;
            let mut bytes = Vec::new();
            for segment in segments {
                match segment {
                    MultipartSegment::Bytes(part) => bytes.extend_from_slice(&part),
                    MultipartSegment::File(_) => {
                        return Err(failure("file multipart requires host transport"));
                    }
                }
            }
            route.headers.insert(
                "content-type".into(),
                format!("multipart/form-data; boundary={boundary}"),
            );
            Some(bytes)
        }
    };
    // The inferred Bytes type avoids adding a direct bytes dependency to hyperchad_app.
    route.body = body.map(|bytes| Arc::new(bytes.into()));
    Ok(route)
}

fn prepare_metadata(
    request: &RequestEffect,
    uri: &str,
    jar: &Mutex<CookieJar>,
    max_bytes: usize,
) -> Result<RouteRequest, Error> {
    if uri.len() > max_bytes
        || request.headers.len() > 256
        || request
            .headers
            .iter()
            .any(|(k, v)| k.len().saturating_add(v.len()) > max_bytes)
    {
        return Err(failure("local request metadata exceeds limit"));
    }
    let mut route = RouteRequest::from_path(uri, RequestInfo::default());
    route.method = request
        .method
        .parse::<Method>()
        .map_err(|_| failure("invalid request method"))?;
    for (name, value) in &request.headers {
        if name.is_empty()
            || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
            || value.contains(['\r', '\n', '\0'])
        {
            return Err(failure("invalid request header"));
        }
        if route
            .headers
            .insert(name.to_ascii_lowercase(), value.clone())
            .is_some()
        {
            return Err(failure("ambiguous duplicate header"));
        }
    }
    // Keep the exact repeated query that RouteRequest.query cannot represent.
    route.headers.insert(URI_HEADER.into(), uri.into());
    route.cookies = jar
        .lock()
        .map_err(|_| failure("local cookie jar poisoned"))?
        .for_path(&route.path, Instant::now());
    // Local-origin cookies are authoritative; never merge an arbitrary Cookie header.
    route.headers.remove("cookie");
    if !route.cookies.is_empty() {
        route.headers.insert(
            "cookie".into(),
            route
                .cookies
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join("; "),
        );
    }
    Ok(route)
}

fn response(
    content: Option<Content>,
    uri: &str,
    jar: &Mutex<CookieJar>,
    max_bytes: usize,
) -> Result<HostResponse, Error> {
    let mut headers = BTreeMap::new();
    let (status, view, body) = match content {
        None => (204, None, Vec::new()),
        Some(Content::View(view)) => {
            jar.lock()
                .map_err(|_| failure("local cookie jar poisoned"))?
                .apply(&view.response.cookies, uri, Instant::now())?;
            (200, Some(*view), Vec::new())
        }
        Some(Content::Raw { data, content_type }) => {
            if data.len() > max_bytes || content_type.contains(['\r', '\n', '\0']) {
                return Err(failure("invalid or excessive local response"));
            }
            headers.insert("content-type".into(), content_type);
            (200, None, data.to_vec())
        }
        #[cfg(feature = "json")]
        Some(Content::Json(value)) => {
            let bytes = serde_json::to_vec(&value).map_err(|e| failure(e.to_string()))?;
            if bytes.len() > max_bytes {
                return Err(failure("local JSON response exceeds limit"));
            }
            headers.insert("content-type".into(), "application/json".into());
            (200, None, bytes)
        }
    };
    // Status is explicit even for empty responses. View metadata is preserved verbatim;
    // navigation is applied by the runtime, never followed automatically by this transport.
    Ok(HostResponse::Route {
        status,
        headers,
        view,
        body,
    })
}

#[derive(Clone)]
struct Cookie {
    value: ResponseCookie,
    expires: Option<Instant>,
}
#[derive(Default)]
struct CookieJar {
    entries: BTreeMap<(String, String), Cookie>,
}
impl CookieJar {
    fn apply(&mut self, cookies: &[ResponseCookie], _uri: &str, now: Instant) -> Result<(), Error> {
        if cookies.len() > MAX_COOKIES {
            return Err(failure("too many response cookies"));
        }
        let mut next = self.entries.clone();
        next.retain(|_, c| c.expires.is_none_or(|at| at > now));
        for cookie in cookies {
            if cookie.name.is_empty()
                || !cookie
                    .name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
                || cookie
                    .value
                    .bytes()
                    .any(|b| b <= 32 || b >= 127 || matches!(b, b';' | b',' | b'"' | b'\\'))
                || !cookie.path.starts_with('/')
                || cookie.path.contains(['\r', '\n', '\0', '?', '#'])
                || cookie
                    .name
                    .len()
                    .saturating_add(cookie.value.len())
                    .saturating_add(cookie.path.len())
                    > MAX_COOKIE_BYTES
                || cookie.same_site == SameSite::None && !cookie.secure
                || cookie.name.starts_with("__Secure-") && !cookie.secure
                || cookie.name.starts_with("__Host-") && (!cookie.secure || cookie.path != "/")
            {
                return Err(failure("invalid local response cookie"));
            }
            let key = (cookie.name.clone(), cookie.path.clone());
            if cookie.max_age_seconds.is_some_and(|age| age <= 0) {
                next.remove(&key);
                continue;
            }
            let expires = cookie
                .max_age_seconds
                .map(|seconds| {
                    u64::try_from(seconds)
                        .ok()
                        .and_then(|s| now.checked_add(Duration::from_secs(s)))
                        .ok_or_else(|| failure("cookie lifetime overflow"))
                })
                .transpose()?;
            next.insert(
                key,
                Cookie {
                    value: cookie.clone(),
                    expires,
                },
            );
            if next.len() > MAX_COOKIES {
                return Err(failure("local cookie jar full"));
            }
        }
        self.entries = next;
        Ok(())
    }
    fn for_path(&mut self, path: &str, now: Instant) -> BTreeMap<String, String> {
        self.entries
            .retain(|_, cookie| cookie.expires.is_none_or(|at| at > now));
        let mut matches: Vec<_> = self
            .entries
            .values()
            .filter(|cookie| {
                let scope = cookie.value.path.as_str();
                path == scope
                    || path.starts_with(scope)
                        && (scope.ends_with('/') || path.as_bytes().get(scope.len()) == Some(&b'/'))
            })
            .collect();
        // BTreeMap cannot encode duplicate cookie names. Prefer the most specific path.
        matches.sort_by_key(|cookie| cookie.value.path.len());
        // This is a private in-process, trusted local origin, not insecure HTTP. Secure and
        // HttpOnly cookies are retained, never sent to remote URLs or exposed to scripts.
        matches
            .into_iter()
            .map(|cookie| (cookie.value.name.clone(), cookie.value.value.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyperchad_renderer::transformer::models::{Selector, SwapStrategy};
    use hyperchad_renderer::{ResponseMetadata, ResponseNavigation, View};
    use hyperchad_renderer_bmux::actions::Source;

    fn request(body: RequestBody) -> RequestEffect {
        RequestEffect {
            source: Source {
                key: "form".into(),
                path: vec![2],
                generation: 7,
            },
            url: "/submit?tag=a&tag=b".into(),
            base_url: "/page".into(),
            method: "POST".into(),
            body,
            headers: BTreeMap::from([("x-test".into(), "retained".into())]),
            target: Selector::Class("target".into()),
            strategy: SwapStrategy::BeforeEnd,
            request_id: Some("request-7".into()),
            navigation: false,
        }
    }
    #[cfg(feature = "assets")]
    fn memory_asset() -> hyperchad_renderer::assets::StaticAssetRoute {
        hyperchad_renderer::assets::StaticAssetRoute {
            route: "/assets/cover art.bin".into(),
            target: hyperchad_renderer::assets::AssetPathTarget::FileContents(
                vec![0, 1, 2, 3].into(),
            ),
            not_found_behavior: None,
        }
    }

    #[cfg(feature = "assets")]
    #[switchy_async::test]
    async fn registered_memory_is_exact_bounded_and_never_reads_files() {
        let config = attach_assets(BmuxConfig::default(), vec![memory_asset()]);
        let callback = config.host.request.unwrap();
        let result = callback(HostRequest::Resource {
            source: "/assets/cover%20art.bin?cache=1".into(),
            max_bytes: 4,
        })
        .await
        .unwrap();
        assert!(matches!(result,HostResponse::Bytes(bytes) if bytes == [0,1,2,3]));
        for (path, bound) in [
            ("/assets/cover%20art.bin", 3),
            ("/assets/cover%20art.bin/child", 100),
            ("/assets/cover%20art.bin-more", 100),
            ("/assets", 100),
        ] {
            assert!(
                callback(HostRequest::Resource {
                    source: path.into(),
                    max_bytes: bound
                })
                .await
                .is_err()
            );
        }
        let config = BmuxConfig {
            max_text_bytes: 3,
            ..BmuxConfig::default()
        };
        let callback = attach_assets(config, vec![memory_asset()])
            .host
            .request
            .unwrap();
        assert!(
            callback(HostRequest::Resource {
                source: "/assets/cover%20art.bin".into(),
                max_bytes: 100
            })
            .await
            .is_err()
        );
    }

    #[cfg(feature = "assets")]
    #[test]
    fn asset_traversal_decoding_and_boundaries_fail_closed() {
        for path in [
            "/assets/../secret",
            "/assets/./cover",
            "/assets/%2e%2e/secret",
            "/assets/%2Fsecret",
            "/assets/%5csecret",
            "/assets/%252e%252e/secret",
            "/assets/%00",
            "/assets/%ff",
            "/assets/%",
            "/assets/%1x",
            "/assets/\\secret",
        ] {
            assert!(asset_path(path).is_err(), "accepted {path}");
        }
        assert_eq!(asset_path("https://example.com/asset").unwrap(), None);
        assert_eq!(asset_path("//example.com/asset").unwrap(), None);
        assert_eq!(
            asset_path("/assets/cover%20art.bin#fragment")
                .unwrap()
                .as_deref(),
            Some("/assets/cover art.bin")
        );
    }

    #[cfg(feature = "assets")]
    #[switchy_async::test]
    async fn asset_fallback_is_explicit_and_receives_original_request_once() {
        use hyperchad_renderer::assets::{AssetPathTarget, StaticAssetRoute};
        let calls = Arc::new(Mutex::new(Vec::new()));
        let seen = calls.clone();
        let mut config = BmuxConfig::default();
        config.host.request = Some(Arc::new(move |request| {
            seen.lock().unwrap().push(request);
            Box::pin(async { Ok(HostResponse::Complete) })
        }));
        let assets = vec![
            memory_asset(),
            StaticAssetRoute {
                route: "/file".into(),
                target: AssetPathTarget::File("/not-opened-by-test".into()),
                not_found_behavior: None,
            },
            StaticAssetRoute {
                route: "/directory".into(),
                target: AssetPathTarget::Directory("/not-opened-by-test".into()),
                not_found_behavior: None,
            },
        ];
        let callback = attach_assets(config, assets).host.request.unwrap();
        for path in [
            "/unknown?original=1",
            "/file",
            "/directory/child",
            "https://example.com/asset",
        ] {
            assert!(matches!(
                callback(HostRequest::Resource {
                    source: path.into(),
                    max_bytes: 17
                })
                .await
                .unwrap(),
                HostResponse::Complete
            ));
        }
        assert_eq!(calls.lock().unwrap().len(), 4);
        assert!(
            matches!(&calls.lock().unwrap()[0],HostRequest::Resource { source,max_bytes:17 } if source == "/unknown?original=1")
        );
        assert!(
            callback(HostRequest::Resource {
                source: "/assets/%2e%2e/secret".into(),
                max_bytes: 100
            })
            .await
            .is_err()
        );
        assert_eq!(calls.lock().unwrap().len(), 4);
        assert!(matches!(
            callback(HostRequest::Resource {
                source: "/assets/cover%20art.bin".into(),
                max_bytes: 4
            })
            .await
            .unwrap(),
            HostResponse::Bytes(_)
        ));
        assert_eq!(calls.lock().unwrap().len(), 4);
    }

    #[test]
    fn repeated_fields_method_headers_and_body_survive() {
        let request = request(RequestBody::UrlEncoded(
            "tag=a&tag=b&text=a+b%26%C3%A9".into(),
        ));
        let jar = Mutex::new(CookieJar::default());
        let route = prepare(&request, &request.url, &jar, 4096).unwrap();
        assert_eq!(route.method, Method::Post);
        assert_eq!(route.headers["x-test"], "retained");
        assert_eq!(route.headers[URI_HEADER], "/submit?tag=a&tag=b");
        assert_eq!(
            &route.body.as_ref().unwrap()[..],
            b"tag=a&tag=b&text=a+b%26%C3%A9"
        );
        assert_eq!(request.strategy, SwapStrategy::BeforeEnd);
        assert_eq!(request.request_id.as_deref(), Some("request-7"));
    }
    #[test]
    fn multipart_text_preserves_duplicate_names() {
        let request = request(RequestBody::Multipart(vec![
            FormPart::Text {
                name: "tag".into(),
                value: "a".into(),
            },
            FormPart::Text {
                name: "tag".into(),
                value: "b".into(),
            },
        ]));
        let route = prepare(&request, "/submit", &Mutex::new(CookieJar::default()), 4096).unwrap();
        assert!(route.headers["content-type"].starts_with("multipart/form-data; boundary="));
        let body = std::str::from_utf8(route.body.as_ref().unwrap()).unwrap();
        assert_eq!(body.matches("name=\"tag\"").count(), 2);
        assert!(body.contains("\r\n\r\na\r\n"));
        assert!(body.contains("\r\n\r\nb\r\n"));
    }
    #[test]
    fn cookie_roundtrip_expiry_path_and_origin_isolation() {
        let jar = Mutex::new(CookieJar::default());
        let now = Instant::now();
        let mut cookie = ResponseCookie::secure("session", "secret");
        cookie.path = "/app".into();
        cookie.max_age_seconds = Some(10);
        jar.lock()
            .unwrap()
            .apply(&[cookie.clone()], "/app/login", now)
            .unwrap();
        let mut request = request(RequestBody::Empty);
        request.url = "/app/page".into();
        let route = prepare(&request, &request.url, &jar, 4096).unwrap();
        assert_eq!(route.cookies["session"], "secret");
        assert_eq!(route.headers["cookie"], "session=secret");
        assert!(jar.lock().unwrap().for_path("/application", now).is_empty());
        assert!(
            jar.lock()
                .unwrap()
                .for_path("/app", now + Duration::from_secs(11))
                .is_empty()
        );
        jar.lock().unwrap().apply(&[cookie], "/app", now).unwrap();
        let mut expired = ResponseCookie::expired("session");
        expired.path = "/app".into();
        jar.lock().unwrap().apply(&[expired], "/app", now).unwrap();
        assert!(jar.lock().unwrap().for_path("/app", now).is_empty());
        assert!(CookieJar::default().for_path("/app", now).is_empty());
    }
    #[test]
    fn view_metadata_and_no_content_status_are_preserved() {
        let navigation = ResponseNavigation::internal("/next").unwrap();
        let view = View {
            response: ResponseMetadata {
                cookies: vec![ResponseCookie::secure("session", "token")],
                navigation: Some(navigation.clone()),
            },
            ..View::default()
        };
        let jar = Mutex::new(CookieJar::default());
        let HostResponse::Route {
            status,
            view: Some(view),
            ..
        } = response(Some(Content::View(Box::new(view))), "/login", &jar, 4096).unwrap()
        else {
            panic!("view");
        };
        assert_eq!(status, 200);
        assert_eq!(view.response.navigation, Some(navigation));
        assert_eq!(view.response.cookies.len(), 1);
        assert_eq!(
            jar.lock().unwrap().for_path("/", Instant::now())["session"],
            "token"
        );
        assert!(matches!(
            response(None, "/", &jar, 4096).unwrap(),
            HostResponse::Route {
                status: 204,
                view: None,
                ..
            }
        ));
    }
    #[test]
    fn external_and_unsafe_urls_never_become_local_routes() {
        for url in [
            "https://example.com/a",
            "http://localhost/a",
            "//example.com/a",
            "file:/secret",
            "javascript:run()",
        ] {
            assert_eq!(local_uri(url, "/").unwrap(), None);
        }
        assert_eq!(
            local_uri("relative", "https://example.com/a").unwrap(),
            None
        );
        assert!(local_uri("/bad\nheader", "/").is_err());
        assert_eq!(
            local_uri("../submit?x=1#fragment", "/app/forms/edit")
                .unwrap()
                .as_deref(),
            Some("/app/submit?x=1")
        );
    }
    #[test]
    fn every_method_body_and_cookie_validation_is_explicit() {
        let jar = Mutex::new(CookieJar::default());
        for (name, method) in [
            ("GET", Method::Get),
            ("POST", Method::Post),
            ("PUT", Method::Put),
            ("PATCH", Method::Patch),
            ("DELETE", Method::Delete),
            ("HEAD", Method::Head),
            ("OPTIONS", Method::Options),
        ] {
            let mut req = request(RequestBody::Empty);
            req.method = name.into();
            assert_eq!(prepare(&req, "/submit", &jar, 4096).unwrap().method, method);
        }
        let mut req = request(RequestBody::Empty);
        req.method = "NOT-A-METHOD".into();
        assert!(prepare(&req, "/submit", &jar, 4096).is_err());
        let mut invalid = ResponseCookie::secure("valid", "value");
        invalid.name = "bad;name".into();
        assert!(
            jar.lock()
                .unwrap()
                .apply(&[invalid], "/", Instant::now())
                .is_err()
        );
        let mut invalid = ResponseCookie::secure("cross", "value");
        invalid.same_site = SameSite::None;
        invalid.secure = false;
        assert!(
            jar.lock()
                .unwrap()
                .apply(&[invalid], "/", Instant::now())
                .is_err()
        );
        assert!(jar.lock().unwrap().for_path("/", Instant::now()).is_empty());
    }

    #[switchy_async::test]
    async fn explicit_policy_receives_cookies_files_and_external_requests_once() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let observed = calls.clone();
        let mut config = BmuxConfig::default();
        config.host.request = Some(Arc::new(move |operation| {
            observed.lock().unwrap().push(operation);
            Box::pin(async { Ok(HostResponse::Complete) })
        }));
        let local_calls = Arc::new(Mutex::new(0));
        let local = local_calls.clone();
        let router = Router::new().with_route_result("/submit", move |_: RouteRequest| {
            *local.lock().unwrap() += 1;
            async { Ok::<_, std::io::Error>(Content::View(Box::default())) }
        });
        let callback = attach(config, router).host.request.unwrap();
        callback(HostRequest::Cookies(vec![ResponseCookie::secure(
            "remote", "value",
        )]))
        .await
        .unwrap();
        callback(HostRequest::ExternalLink {
            url: "https://example.com".into(),
        })
        .await
        .unwrap();
        let file = hyperchad_renderer_bmux::actions::AuthorizedFile {
            token: "opaque-host-capability".into(),
            name: "upload.txt".into(),
            content_type: "text/plain".into(),
            size: 3,
        };
        callback(HostRequest::Http(Box::new(request(
            RequestBody::Multipart(vec![FormPart::File {
                name: "file".into(),
                file,
            }]),
        ))))
        .await
        .unwrap();
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 3);
        assert!(matches!(&calls[0],HostRequest::Cookies(cookies) if cookies[0].name == "remote"));
        assert!(
            matches!(&calls[1],HostRequest::ExternalLink { url } if url == "https://example.com")
        );
        assert!(
            matches!(&calls[2],HostRequest::Http(req) if req.request_id.as_deref() == Some("request-7") && req.strategy == SwapStrategy::BeforeEnd)
        );
        drop(calls);
        assert_eq!(*local_calls.lock().unwrap(), 0);
    }

    #[switchy_async::test]
    async fn cookie_jar_roundtrips_between_real_local_route_invocations() {
        let observations = Arc::new(Mutex::new(Vec::new()));
        let seen = observations.clone();
        let router = Router::new().with_route_result("/submit", move |req: RouteRequest| {
            let previous = req.cookies.get("session").cloned();
            seen.lock().unwrap().push(previous);
            async {
                Ok::<_, std::io::Error>(Content::View(Box::new(View {
                    response: ResponseMetadata {
                        cookies: vec![ResponseCookie::secure("session", "local-secret")],
                        navigation: None,
                    },
                    ..View::default()
                })))
            }
        });
        let callback = attach(BmuxConfig::default(), router).host.request.unwrap();
        for _ in 0..2 {
            callback(HostRequest::Http(Box::new(request(RequestBody::Empty))))
                .await
                .unwrap();
        }
        assert_eq!(
            *observations.lock().unwrap(),
            vec![None, Some("local-secret".into())]
        );
        assert!(matches!(
            callback(HostRequest::Cookies(vec![])).await.unwrap(),
            HostResponse::Complete
        ));
    }

    #[switchy_async::test]
    async fn local_handler_executes_once_without_launching_external_operations() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let seen = calls.clone();
        let router = Router::new().with_route_result("/submit", move |request: RouteRequest| {
            seen.lock().unwrap().push(request);
            async { Ok::<_, std::io::Error>(Content::View(Box::default())) }
        });
        let config = attach(BmuxConfig::default(), router);
        let callback = config.host.request.unwrap();
        let result = callback(HostRequest::Http(Box::new(request(
            RequestBody::UrlEncoded("tag=a&tag=b".into()),
        ))))
        .await
        .unwrap();
        assert!(matches!(result, HostResponse::Route { status: 200, .. }));
        assert_eq!(calls.lock().unwrap().len(), 1);
        assert!(
            callback(HostRequest::ExternalLink {
                url: "https://example.com".into()
            })
            .await
            .is_err()
        );
        assert!(
            callback(HostRequest::Resource {
                source: "/arbitrary/file".into(),
                max_bytes: 100
            })
            .await
            .is_err()
        );
        assert_eq!(calls.lock().unwrap().len(), 1);
    }
}
