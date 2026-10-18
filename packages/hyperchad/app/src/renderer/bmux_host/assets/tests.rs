use super::super::{attach, attach_assets};
use super::*;
use hyperchad_renderer::assets::AssetNotFoundBehavior;
use hyperchad_renderer_bmux::renderer::{BmuxConfig, HostRequest};
use std::sync::Mutex;

struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "hyperchad-bmux-assets-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn mount(route: &str, target: AssetPathTarget) -> StaticAssetRoute {
    StaticAssetRoute {
        route: route.into(),
        target,
        not_found_behavior: None,
    }
}

fn request(source: &str, max_bytes: usize) -> HostRequest {
    HostRequest::Resource {
        source: source.into(),
        max_bytes,
    }
}

#[switchy_async::test]
async fn native_public_mount_serves_real_png_and_svg_through_route_adapter() {
    let public = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/native/public");
    let png = std::fs::read(public.join("img/icon128.png")).unwrap();
    let svg = std::fs::read(public.join("img/album.svg")).unwrap();
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(std::str::from_utf8(&svg).unwrap().contains("<svg"));
    let config = attach_assets(
        BmuxConfig::default(),
        vec![mount("/public", AssetPathTarget::Directory(public))],
    );
    let callback = attach(config, hyperchad_router::Router::new())
        .host
        .request
        .unwrap();
    for (source, bytes) in [
        ("/public/img/icon128.png?cache=1#image", png),
        ("/public/img/album.svg", svg),
    ] {
        assert!(
            matches!(callback(request(source, bytes.len())).await.unwrap(), HostResponse::Bytes(result) if result == bytes)
        );
    }
}

#[switchy_async::test]
async fn registered_file_and_directory_decoding_boundaries_and_limits() {
    let root = TempRoot::new();
    std::fs::write(root.0.join("cover art.svg"), b"<svg/>").unwrap();
    std::fs::write(root.0.join("empty"), b"").unwrap();
    let callback = attach_assets(
        BmuxConfig::default(),
        vec![
            mount("/one", AssetPathTarget::File(root.0.join("cover art.svg"))),
            mount("/public", AssetPathTarget::Directory(root.0.clone())),
        ],
    )
    .host
    .request
    .unwrap();
    for source in [
        "/one",
        "/public/cover%20art.svg?rev=2",
        "/public/cover art.svg#img",
    ] {
        assert!(
            matches!(callback(request(source, 6)).await.unwrap(), HostResponse::Bytes(bytes) if bytes == b"<svg/>")
        );
        assert!(callback(request(source, 5)).await.is_err());
        assert!(callback(request(source, 0)).await.is_err());
    }
    assert!(
        matches!(callback(request("/public/empty", 0)).await.unwrap(), HostResponse::Bytes(bytes) if bytes.is_empty())
    );
    for source in [
        "/one/child",
        "/one-more",
        "/publicity/cover%20art.svg",
        "file:///etc/passwd",
        "/public/../cover%20art.svg",
        "/public/%2e%2e/secret",
        "/public/%252e%252e/secret",
        "/public/%2fetc/passwd",
        "/public/%5csecret",
        "/public/%00",
        "/public/%ff",
        "/public/%",
    ] {
        assert!(
            callback(request(source, 100)).await.is_err(),
            "accepted {source}"
        );
    }
    let callback = attach_assets(
        BmuxConfig {
            max_text_bytes: 5,
            ..BmuxConfig::default()
        },
        vec![mount(
            "/one",
            AssetPathTarget::File(root.0.join("cover art.svg")),
        )],
    )
    .host
    .request
    .unwrap();
    assert!(callback(request("/one", 100)).await.is_err());
}

#[switchy_async::test]
async fn matching_missing_and_nonregular_fail_closed_except_explicit_not_found_fallthrough() {
    let root = TempRoot::new();
    std::fs::create_dir(root.0.join("directory")).unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let seen = calls.clone();
    let mut config = BmuxConfig::default();
    config.host.request = Some(Arc::new(move |operation| {
        seen.lock().unwrap().push(operation);
        Box::pin(async { Ok(HostResponse::Complete) })
    }));
    let mut fallthrough = mount("/fallback", AssetPathTarget::Directory(root.0.clone()));
    fallthrough.not_found_behavior = Some(AssetNotFoundBehavior::Fallthrough);
    let mut error = mount("/error", AssetPathTarget::Directory(root.0.clone()));
    error.not_found_behavior = Some(AssetNotFoundBehavior::InternalServerError);
    let callback = attach_assets(
        config,
        vec![
            mount("/public", AssetPathTarget::Directory(root.0.clone())),
            mount(
                "/missing-file",
                AssetPathTarget::File(root.0.join("missing")),
            ),
            mount(
                "/not-a-file",
                AssetPathTarget::File(root.0.join("directory")),
            ),
            fallthrough,
            error,
        ],
    )
    .host
    .request
    .unwrap();
    for source in [
        "/public/missing",
        "/public/directory",
        "/missing-file",
        "/not-a-file",
        "/error/missing",
        "/fallback/directory",
        "/fallback/%2e%2e/secret",
    ] {
        assert!(
            callback(request(source, 100)).await.is_err(),
            "accepted {source}"
        );
    }
    assert!(calls.lock().unwrap().is_empty());
    for source in [
        "/fallback/missing?keep=1",
        "/publicity/directory",
        "https://example.com/cover",
        "file:///etc/passwd",
    ] {
        assert!(matches!(
            callback(request(source, 100)).await.unwrap(),
            HostResponse::Complete
        ));
    }
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 4);
    assert!(
        matches!(&calls[0], HostRequest::Resource { source, max_bytes:100 } if source == "/fallback/missing?keep=1")
    );
}

#[switchy_async::test]
async fn registration_order_is_preserved_across_asset_kinds() {
    let root = TempRoot::new();
    std::fs::write(root.0.join("image"), b"disk").unwrap();
    let callback = attach_assets(
        BmuxConfig::default(),
        vec![
            mount(
                "/public/image",
                AssetPathTarget::FileContents(b"memory".to_vec().into()),
            ),
            mount("/public", AssetPathTarget::Directory(root.0.clone())),
            mount(
                "/public/image",
                AssetPathTarget::FileContents(b"late".to_vec().into()),
            ),
        ],
    )
    .host
    .request
    .unwrap();
    assert!(
        matches!(callback(request("/public/image", 6)).await.unwrap(), HostResponse::Bytes(bytes) if bytes == b"memory")
    );
}

#[test]
fn bounds_are_checked_on_reader_not_metadata_and_include_sentinel() {
    struct Endless(Arc<AtomicUsize>);
    impl Read for Endless {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.0.fetch_add(buf.len(), Ordering::Relaxed);
            buf.fill(42);
            Ok(buf.len())
        }
    }
    let count = Arc::new(AtomicUsize::new(0));
    assert!(read_bounded(Endless(count.clone()), 31).is_err());
    assert_eq!(count.load(Ordering::Relaxed), 32);
    assert_eq!(read_bounded(&b"exact"[..], 5).unwrap(), b"exact");
    assert!(read_bounded(&b"larger"[..], 5).is_err());
}

#[cfg(unix)]
#[switchy_async::test]
async fn symlinks_cannot_escape_and_replacing_root_path_does_not_retarget_capability() {
    use std::os::unix::fs::symlink;
    let root = TempRoot::new();
    let outside = TempRoot::new();
    std::fs::write(root.0.join("safe"), b"safe").unwrap();
    std::fs::write(outside.0.join("secret"), b"secret").unwrap();
    symlink(outside.0.join("secret"), root.0.join("escape-file")).unwrap();
    symlink(&outside.0, root.0.join("escape-dir")).unwrap();
    symlink("safe", root.0.join("inside")).unwrap();
    let callback = attach_assets(
        BmuxConfig::default(),
        vec![
            mount("/public", AssetPathTarget::Directory(root.0.clone())),
            mount("/file", AssetPathTarget::File(root.0.join("escape-file"))),
        ],
    )
    .host
    .request
    .unwrap();
    assert!(
        matches!(callback(request("/public/inside", 10)).await.unwrap(), HostResponse::Bytes(bytes) if bytes == b"safe")
    );
    for source in ["/public/escape-file", "/public/escape-dir/secret", "/file"] {
        assert!(
            callback(request(source, 100)).await.is_err(),
            "escaped {source}"
        );
    }
    let moved = root.0.with_extension("moved");
    std::fs::rename(&root.0, &moved).unwrap();
    symlink(&outside.0, &root.0).unwrap();
    assert!(
        matches!(callback(request("/public/safe", 10)).await.unwrap(), HostResponse::Bytes(bytes) if bytes == b"safe")
    );
    assert!(callback(request("/public/secret", 10)).await.is_err());
    // Restore the test-owned root for scoped cleanup.
    std::fs::remove_file(&root.0).unwrap();
    std::fs::rename(moved, &root.0).unwrap();
}

#[cfg(unix)]
#[switchy_async::test]
async fn fifo_is_rejected_without_waiting_for_a_writer() {
    let root = TempRoot::new();
    assert!(
        std::process::Command::new("mkfifo")
            .arg(root.0.join("pipe"))
            .status()
            .unwrap()
            .success()
    );
    let callback = attach_assets(
        BmuxConfig::default(),
        vec![mount("/public", AssetPathTarget::Directory(root.0.clone()))],
    )
    .host
    .request
    .unwrap();
    assert!(callback(request("/public/pipe", 100)).await.is_err());
}

#[switchy_async::test]
async fn missing_file_can_appear_later_and_concurrency_is_bounded() {
    let root = TempRoot::new();
    let assets = Arc::new(Assets::new(vec![mount(
        "/public",
        AssetPathTarget::Directory(root.0.clone()),
    )]));
    assert!(assets.load("/public/later", 4).await.is_err());
    std::fs::write(root.0.join("later"), b"live").unwrap();
    assert!(
        matches!(assets.load("/public/later", 4).await.unwrap(), Some(HostResponse::Bytes(bytes)) if bytes == b"live")
    );
    assets.active.store(MAX_READS, Ordering::Release);
    assert!(assets.load("/public/later", 4).await.is_err());
    assert_eq!(assets.active.load(Ordering::Acquire), MAX_READS);
    let slot = ReadSlot(assets.active.clone());
    drop(slot);
    assert!(assets.load("/public/later", 4).await.unwrap().is_some());
    assert_eq!(assets.active.load(Ordering::Acquire), MAX_READS - 1);
}

#[switchy_async::test]
async fn filesystem_reads_have_an_independent_hard_byte_cap() {
    let root = TempRoot::new();
    let file = std::fs::File::create(root.0.join("large")).unwrap();
    file.set_len(u64::try_from(MAX_ASSET_BYTES + 1).unwrap())
        .unwrap();
    let callback = attach_assets(
        BmuxConfig {
            max_text_bytes: usize::MAX,
            ..BmuxConfig::default()
        },
        vec![mount("/public", AssetPathTarget::Directory(root.0.clone()))],
    )
    .host
    .request
    .unwrap();
    assert!(
        callback(request("/public/large", usize::MAX))
            .await
            .is_err()
    );
}
