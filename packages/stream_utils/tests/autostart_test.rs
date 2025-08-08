use bytes::Bytes;
use moosicbox_stream_utils::remote_bytestream::{HttpFetcher, RemoteByteStream};
use switchy_async::util::CancellationToken;

#[derive(Clone)]
struct TestHttpFetcher {
    data: Vec<u8>,
}

impl TestHttpFetcher {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}

#[async_trait::async_trait]
impl HttpFetcher for TestHttpFetcher {
    async fn fetch_range(
        &self,
        _url: &str,
        _start: u64,
        _end: Option<u64>,
    ) -> Result<
        Box<
            dyn futures::Stream<Item = Result<Bytes, Box<dyn std::error::Error + Send + Sync>>>
                + Send
                + Unpin,
        >,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        println!("TestHttpFetcher::fetch_range called");
        let data = Bytes::from(self.data.clone());
        let stream = futures::stream::once(async move {
            println!("TestHttpFetcher: returning data");
            Ok(data)
        });
        Ok(Box::new(Box::pin(stream)))
    }
}

#[switchy_async::test]
async fn autostart_test() {
    println!("Autostart test starting");

    let abort_token = CancellationToken::new();
    println!("Created abort token");

    let fetcher = TestHttpFetcher::new(b"hello world".to_vec());
    println!("Created fetcher");

    let _stream = RemoteByteStream::new_with_fetcher(
        "https://example.com/file.mp3".to_string(),
        Some(11), // Total size: 11 bytes
        true,     // Auto-start fetch
        true,     // Seekable
        abort_token,
        fetcher,
    );
    println!("Created stream with auto-start");

    println!("Test completed successfully");
}
