# MoosicBox Tunnel

A secure tunneling system for the MoosicBox ecosystem, providing encrypted communication channels, NAT traversal, and remote access capabilities for connecting distributed music services and clients.

## Features

- **Secure Tunneling**: End-to-end encrypted tunnels using modern cryptography
- **NAT Traversal**: Automatic NAT and firewall traversal for peer-to-peer connections
- **WebSocket Support**: WebSocket-based tunneling for web client compatibility
- **Multiple Protocols**: Support for TCP, UDP, and WebSocket tunneling
- **Authentication**: Built-in authentication and authorization mechanisms
- **Load Balancing**: Distribute traffic across multiple tunnel endpoints
- **Connection Pooling**: Efficient connection management and reuse
- **Automatic Reconnection**: Robust reconnection logic with exponential backoff
- **Bandwidth Management**: Traffic shaping and bandwidth limiting
- **Monitoring & Metrics**: Real-time tunnel performance monitoring

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_tunnel = "0.1.1"
```

## Usage

### Basic Tunnel Setup

```rust
use moosicbox_tunnel::{TunnelServer, TunnelClient, TunnelConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Server side - create tunnel server
    let server_config = TunnelConfig {
        bind_address: "0.0.0.0:8080".to_string(),
        encryption_key: "your-secret-key".to_string(),
        max_connections: 100,
        idle_timeout: Duration::from_secs(300),
        enable_compression: true,
    };

    let tunnel_server = TunnelServer::new(server_config).await?;

    // Start server
    tokio::spawn(async move {
        tunnel_server.run().await.unwrap();
    });

    // Client side - connect to tunnel
    let client_config = TunnelConfig {
        bind_address: "server.example.com:8080".to_string(),
        encryption_key: "your-secret-key".to_string(),
        max_connections: 10,
        idle_timeout: Duration::from_secs(300),
        enable_compression: true,
    };

    let tunnel_client = TunnelClient::new(client_config).await?;
    let connection = tunnel_client.connect().await?;

    println!("Tunnel established successfully");

    Ok(())
}
```

### WebSocket Tunneling

```rust
use moosicbox_tunnel::{WebSocketTunnel, TunnelMessage, MessageType};

async fn websocket_tunnel() -> Result<(), Box<dyn std::error::Error>> {
    let tunnel = WebSocketTunnel::new("wss://tunnel.example.com/ws").await?;

    // Send data through tunnel
    let message = TunnelMessage {
        message_type: MessageType::Data,
        payload: b"Hello through tunnel".to_vec(),
        destination: Some("target-service".to_string()),
        source: Some("client-1".to_string()),
    };

    tunnel.send_message(message).await?;

    // Receive messages
    while let Some(message) = tunnel.receive_message().await? {
        match message.message_type {
            MessageType::Data => {
                println!("Received data: {}", String::from_utf8_lossy(&message.payload));
            }
            MessageType::Control => {
                println!("Received control message");
            }
            MessageType::Heartbeat => {
                // Handle heartbeat
            }
        }
    }

    Ok(())
}
```

### Secure API Tunneling

```rust
use moosicbox_tunnel::{ApiTunnel, HttpRequest, HttpResponse};

async fn api_tunnel() -> Result<(), Box<dyn std::error::Error>> {
    let api_tunnel = ApiTunnel::new("https://api.example.com").await?;

    // Forward HTTP requests through tunnel
    let request = HttpRequest {
        method: "GET".to_string(),
        path: "/api/tracks".to_string(),
        headers: vec![
            ("Authorization".to_string(), "Bearer token".to_string()),
            ("Content-Type".to_string(), "application/json".to_string()),
        ],
        body: None,
    };

    let response = api_tunnel.forward_request(request).await?;

    match response.status_code {
        200 => {
            println!("API request successful");
            if let Some(body) = response.body {
                println!("Response: {}", String::from_utf8_lossy(&body));
            }
        }
        _ => {
            println!("API request failed: {}", response.status_code);
        }
    }

    Ok(())
}
```

### NAT Traversal

```rust
use moosicbox_tunnel::{NATTraversal, STUNConfig, TURNConfig};

async fn nat_traversal() -> Result<(), Box<dyn std::error::Error>> {
    let stun_config = STUNConfig {
        servers: vec![
            "stun:stun.l.google.com:19302".to_string(),
            "stun:stun1.l.google.com:19302".to_string(),
        ],
        timeout: Duration::from_secs(5),
    };

    let turn_config = TURNConfig {
        server: "turn:turn.example.com:3478".to_string(),
        username: "user".to_string(),
        password: "pass".to_string(),
        realm: "example.com".to_string(),
    };

    let nat_traversal = NATTraversal::new(stun_config, Some(turn_config)).await?;

    // Discover external IP and port
    let external_addr = nat_traversal.discover_external_address().await?;
    println!("External address: {}", external_addr);

    // Attempt hole punching
    let peer_addr = "192.168.1.100:8080".parse()?;
    let connection = nat_traversal.punch_hole(peer_addr).await?;

    println!("NAT traversal successful, connection established");

    Ok(())
}
```

### Tunnel with Authentication

```rust
use moosicbox_tunnel::{AuthenticatedTunnel, AuthMethod, Credentials};

async fn authenticated_tunnel() -> Result<(), Box<dyn std::error::Error>> {
    let credentials = Credentials {
        username: "user123".to_string(),
        password: Some("password123".to_string()),
        token: None,
        certificate: None,
    };

    let auth_method = AuthMethod::UsernamePassword {
        hash_algorithm: "SHA256".to_string(),
        salt: "random-salt".to_string(),
    };

    let tunnel = AuthenticatedTunnel::new(
        "tunnel.example.com:8080",
        auth_method,
        credentials,
    ).await?;

    // Tunnel is now authenticated and ready for use
    let connection = tunnel.establish_connection().await?;

    println!("Authenticated tunnel established");

    Ok(())
}
```

### Load Balanced Tunneling

```rust
use moosicbox_tunnel::{TunnelPool, LoadBalancingStrategy, HealthCheck};

async fn load_balanced_tunnel() -> Result<(), Box<dyn std::error::Error>> {
    let tunnel_endpoints = vec![
        "tunnel1.example.com:8080".to_string(),
        "tunnel2.example.com:8080".to_string(),
        "tunnel3.example.com:8080".to_string(),
    ];

    let health_check = HealthCheck {
        interval: Duration::from_secs(30),
        timeout: Duration::from_secs(5),
        path: Some("/health".to_string()),
    };

    let tunnel_pool = TunnelPool::new(
        tunnel_endpoints,
        LoadBalancingStrategy::RoundRobin,
        Some(health_check),
    ).await?;

    // Get connection from pool
    let connection = tunnel_pool.get_connection().await?;

    // Send data through load-balanced tunnel
    connection.send_data(b"Hello from load-balanced tunnel").await?;

    Ok(())
}
```

## Programming Interface

### Core Types

```rust
pub struct TunnelServer {
    config: TunnelConfig,
    listener: TcpListener,
    connections: Arc<Mutex<HashMap<String, TunnelConnection>>>,
    metrics: TunnelMetrics,
}

impl TunnelServer {
    pub async fn new(config: TunnelConfig) -> Result<Self, TunnelError>;
    pub async fn run(&self) -> Result<(), TunnelError>;
    pub async fn stop(&self) -> Result<(), TunnelError>;
    pub fn get_metrics(&self) -> TunnelMetrics;
    pub async fn broadcast_message(&self, message: TunnelMessage) -> Result<(), TunnelError>;
}

pub struct TunnelClient {
    config: TunnelConfig,
    connection: Option<TunnelConnection>,
    reconnect_strategy: ReconnectStrategy,
}

impl TunnelClient {
    pub async fn new(config: TunnelConfig) -> Result<Self, TunnelError>;
    pub async fn connect(&mut self) -> Result<&TunnelConnection, TunnelError>;
    pub async fn disconnect(&mut self) -> Result<(), TunnelError>;
    pub async fn send_message(&self, message: TunnelMessage) -> Result<(), TunnelError>;
    pub async fn receive_message(&self) -> Result<Option<TunnelMessage>, TunnelError>;
}

#[derive(Debug, Clone)]
pub struct TunnelConfig {
    pub bind_address: String,
    pub encryption_key: String,
    pub max_connections: usize,
    pub idle_timeout: Duration,
    pub enable_compression: bool,
    pub buffer_size: usize,
    pub heartbeat_interval: Duration,
}
```

### Message Types

```rust
#[derive(Debug, Clone)]
pub struct TunnelMessage {
    pub message_type: MessageType,
    pub payload: Vec<u8>,
    pub destination: Option<String>,
    pub source: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub message_id: String,
}

#[derive(Debug, Clone)]
pub enum MessageType {
    Data,
    Control,
    Heartbeat,
    Authentication,
    Error,
}

pub trait TunnelConnection: Send + Sync {
    async fn send_data(&self, data: &[u8]) -> Result<(), TunnelError>;
    async fn receive_data(&self) -> Result<Vec<u8>, TunnelError>;
    async fn close(&self) -> Result<(), TunnelError>;
    fn is_connected(&self) -> bool;
    fn get_peer_address(&self) -> Option<SocketAddr>;
}
```

### Authentication

```rust
#[derive(Debug, Clone)]
pub enum AuthMethod {
    None,
    UsernamePassword {
        hash_algorithm: String,
        salt: String,
    },
    Token {
        algorithm: String,
        public_key: Vec<u8>,
    },
    Certificate {
        ca_cert: Vec<u8>,
        verify_hostname: bool,
    },
}

#[derive(Debug, Clone)]
pub struct Credentials {
    pub username: String,
    pub password: Option<String>,
    pub token: Option<String>,
    pub certificate: Option<Vec<u8>>,
}

pub trait Authenticator: Send + Sync {
    async fn authenticate(&self, credentials: &Credentials) -> Result<AuthResult, TunnelError>;
    async fn validate_token(&self, token: &str) -> Result<bool, TunnelError>;
}
```

### NAT Traversal

```rust
pub struct NATTraversal {
    stun_config: STUNConfig,
    turn_config: Option<TURNConfig>,
    local_candidates: Vec<IceCandidate>,
}

impl NATTraversal {
    pub async fn new(stun_config: STUNConfig, turn_config: Option<TURNConfig>) -> Result<Self, TunnelError>;
    pub async fn discover_external_address(&self) -> Result<SocketAddr, TunnelError>;
    pub async fn gather_candidates(&self) -> Result<Vec<IceCandidate>, TunnelError>;
    pub async fn punch_hole(&self, peer_addr: SocketAddr) -> Result<TunnelConnection, TunnelError>;
}

#[derive(Debug, Clone)]
pub struct STUNConfig {
    pub servers: Vec<String>,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct TURNConfig {
    pub server: String,
    pub username: String,
    pub password: String,
    pub realm: String,
}
```

## Configuration

### Environment Variables

- `TUNNEL_BIND_ADDRESS`: Default bind address for tunnel server
- `TUNNEL_ENCRYPTION_KEY`: Default encryption key for tunnels
- `TUNNEL_MAX_CONNECTIONS`: Maximum concurrent connections (default: 100)
- `TUNNEL_IDLE_TIMEOUT_SECONDS`: Connection idle timeout (default: 300)
- `TUNNEL_ENABLE_COMPRESSION`: Enable data compression (default: true)
- `TUNNEL_LOG_LEVEL`: Logging level for tunnel operations

### Advanced Configuration

```rust
use moosicbox_tunnel::{TunnelConfig, EncryptionConfig, CompressionConfig};

let config = TunnelConfig {
    bind_address: "0.0.0.0:8080".to_string(),
    encryption_key: "your-secret-key".to_string(),
    max_connections: 500,
    idle_timeout: Duration::from_secs(600),
    enable_compression: true,
    buffer_size: 64 * 1024, // 64KB
    heartbeat_interval: Duration::from_secs(30),

    encryption: EncryptionConfig {
        algorithm: "AES-256-GCM".to_string(),
        key_rotation_interval: Duration::from_secs(3600),
        enable_perfect_forward_secrecy: true,
    },

    compression: CompressionConfig {
        algorithm: "ZSTD".to_string(),
        level: 6,
        enable_adaptive: true,
        min_compress_size: 1024,
    },

    networking: NetworkConfig {
        tcp_nodelay: true,
        tcp_keepalive: Some(Duration::from_secs(60)),
        socket_buffer_size: 128 * 1024,
        enable_ipv6: true,
    },
};
```

## Security Features

### Encryption

```rust
use moosicbox_tunnel::{EncryptionManager, KeyExchange};

// Setup end-to-end encryption
let encryption_manager = EncryptionManager::new("AES-256-GCM")?;

// Perform key exchange
let key_exchange = KeyExchange::new();
let (public_key, private_key) = key_exchange.generate_keypair()?;

// Exchange keys with peer
let shared_secret = key_exchange.derive_shared_secret(&peer_public_key, &private_key)?;

// Create encrypted tunnel
let encrypted_tunnel = tunnel.with_encryption(shared_secret).await?;
```

### Certificate-Based Authentication

```rust
use moosicbox_tunnel::{CertificateAuth, X509Certificate};

let ca_cert = std::fs::read("ca-cert.pem")?;
let client_cert = std::fs::read("client-cert.pem")?;
let client_key = std::fs::read("client-key.pem")?;

let cert_auth = CertificateAuth::new(ca_cert)?;
let credentials = Credentials {
    username: "client".to_string(),
    password: None,
    token: None,
    certificate: Some(client_cert),
};

let tunnel = AuthenticatedTunnel::new(
    "tunnel.example.com:8080",
    AuthMethod::Certificate {
        ca_cert,
        verify_hostname: true,
    },
    credentials,
).await?;
```

## Integration Examples

### MoosicBox Server Tunneling

```rust
use moosicbox_tunnel::{TunnelServer, MoosicBoxTunnelHandler};

struct MusicApiTunnelHandler;

impl TunnelHandler for MusicApiTunnelHandler {
    async fn handle_message(&self, message: TunnelMessage) -> Result<Option<TunnelMessage>, TunnelError> {
        match message.message_type {
            MessageType::Data => {
                // Parse API request from tunnel
                let api_request: ApiRequest = serde_json::from_slice(&message.payload)?;

                // Process request
                let api_response = process_music_api_request(api_request).await?;

                // Send response back through tunnel
                let response_message = TunnelMessage {
                    message_type: MessageType::Data,
                    payload: serde_json::to_vec(&api_response)?,
                    destination: message.source,
                    source: Some("music-api".to_string()),
                    timestamp: Utc::now(),
                    message_id: generate_message_id(),
                };

                Ok(Some(response_message))
            }
            _ => Ok(None),
        }
    }
}

let tunnel_server = TunnelServer::new(config)
    .with_handler(Box::new(MusicApiTunnelHandler))
    .await?;

tunnel_server.run().await?;
```

### Remote Music Streaming

```rust
use moosicbox_tunnel::{StreamingTunnel, AudioStream};

async fn stream_music_through_tunnel() -> Result<(), Box<dyn std::error::Error>> {
    let tunnel = StreamingTunnel::new("music-server.example.com:8080").await?;

    // Request music stream
    let stream_request = StreamRequest {
        track_id: "track_123".to_string(),
        quality: AudioQuality::High,
        start_position: Some(30.0),
    };

    let audio_stream = tunnel.request_stream(stream_request).await?;

    // Stream audio data through tunnel
    while let Some(audio_chunk) = audio_stream.next().await {
        match audio_chunk {
            Ok(data) => {
                // Play audio chunk
                audio_output.play(&data).await?;
            }
            Err(e) => {
                eprintln!("Streaming error: {}", e);
                break;
            }
        }
    }

    Ok(())
}
```

## Monitoring & Metrics

```rust
use moosicbox_tunnel::{TunnelMetrics, MetricsCollector};

// Get tunnel metrics
let metrics = tunnel_server.get_metrics();

println!("Tunnel Metrics:");
println!("  Active connections: {}", metrics.active_connections);
println!("  Total bytes sent: {}", metrics.bytes_sent);
println!("  Total bytes received: {}", metrics.bytes_received);
println!("  Average latency: {:.2}ms", metrics.average_latency.as_secs_f64() * 1000.0);
println!("  Connection errors: {}", metrics.connection_errors);
println!("  Authentication failures: {}", metrics.auth_failures);

// Export metrics to Prometheus
let metrics_collector = MetricsCollector::new();
let prometheus_metrics = metrics_collector.export_prometheus(&metrics)?;
```

## Error Handling

```rust
use moosicbox_tunnel::TunnelError;

match tunnel_client.connect().await {
    Ok(connection) => {
        println!("Tunnel connected successfully");
    }
    Err(TunnelError::ConnectionFailed(addr)) => {
        eprintln!("Failed to connect to tunnel server: {}", addr);
    }
    Err(TunnelError::AuthenticationFailed) => {
        eprintln!("Tunnel authentication failed");
    }
    Err(TunnelError::EncryptionError(e)) => {
        eprintln!("Tunnel encryption error: {}", e);
    }
    Err(TunnelError::NetworkError(e)) => {
        eprintln!("Network error: {}", e);
    }
    Err(e) => eprintln!("Tunnel error: {}", e),
}
```

## Testing

```bash
# Run all tests
cargo test

# Run integration tests
cargo test --test integration

# Test NAT traversal (requires network)
cargo test nat_traversal_tests -- --ignored

# Performance tests
cargo test --release performance_tests -- --ignored
```

## See Also

- [`moosicbox_tunnel_server`](../tunnel_server/README.md) - Tunnel server implementation
- [`moosicbox_tunnel_sender`](../tunnel_sender/README.md) - Tunnel client sender
- [`moosicbox_ws`](../ws/README.md) - WebSocket communication
- [`moosicbox_auth`](../auth/README.md) - Authentication and authorization
- [`switchy_tcp`](../tcp/README.md) - TCP networking utilities
