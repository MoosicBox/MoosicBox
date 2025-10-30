# API Integration Example

A comprehensive example demonstrating actix-web integration with `moosicbox_profiles`, showing how to extract and validate profile information from HTTP requests.

## What This Example Demonstrates

- Using `ProfileName` extractor for verified profiles
- Using `ProfileNameUnverified` extractor for unverified profiles
- Extracting profiles from HTTP headers (`moosicbox-profile`)
- Extracting profiles from query parameters (`moosicboxProfile`)
- Building REST API endpoints that require profile information
- Handling missing or invalid profile errors

## Prerequisites

- Rust 1.75 or later
- Understanding of actix-web basics
- `curl` or similar HTTP client for testing (optional)

## Running the Example

```bash
cargo run --manifest-path packages/profiles/examples/api_integration/Cargo.toml
```

The server will start on `http://127.0.0.1:8080`.

## Expected Output

When starting the server:

```
=== MoosicBox Profiles - API Integration Example ===

Pre-registering test profiles...
Registered: alice, bob, admin

Starting HTTP server on http://127.0.0.1:8080

Available endpoints:
  GET  /verified   - Requires verified profile (must exist in registry)
  GET  /unverified - Accepts any profile name
  GET  /profiles   - Lists all registered profiles
  POST /register   - Registers a new profile

Profile can be provided via:
  - Header: moosicbox-profile: <name>
  - Query param: ?moosicboxProfile=<name>

Example requests:
  curl -H 'moosicbox-profile: alice' http://127.0.0.1:8080/verified
  curl 'http://127.0.0.1:8080/verified?moosicboxProfile=alice'
  curl 'http://127.0.0.1:8080/unverified?moosicboxProfile=newuser'
  curl http://127.0.0.1:8080/profiles
  curl -X POST -H 'moosicbox-profile: charlie' http://127.0.0.1:8080/register

Press Ctrl+C to stop the server
```

## Code Walkthrough

### Setting Up API Integration

```rust
use moosicbox_profiles::api::{ProfileName, ProfileNameUnverified};
```

The `api` module provides two extractors for use with actix-web:

- `ProfileName` - Validates that the profile exists in the registry
- `ProfileNameUnverified` - Extracts the profile name without validation

### Verified Profile Handler

```rust
async fn verified_handler(profile: ProfileName) -> Result<HttpResponse> {
    let profile_name = profile.as_ref();
    Ok(HttpResponse::Ok().json(/* ... */))
}
```

The `ProfileName` extractor automatically:

1. Extracts the profile name from query params or headers
2. Validates that the profile exists in the `PROFILES` registry
3. Returns `400 Bad Request` if missing or non-existent
4. Provides the verified profile name to your handler

### Unverified Profile Handler

```rust
async fn unverified_handler(profile: ProfileNameUnverified) -> Result<HttpResponse> {
    let profile_name: String = profile.into();
    Ok(HttpResponse::Ok().json(/* ... */))
}
```

The `ProfileNameUnverified` extractor:

1. Extracts the profile name from query params or headers
2. Returns the name without checking the registry
3. Returns `400 Bad Request` only if the profile info is missing from the request
4. Useful for registration endpoints or operations that don't require existing profiles

### Profile Extraction Order

Both extractors check for profile information in this order:

1. Query parameter `moosicboxProfile` (checked first)
2. HTTP header `moosicbox-profile` (fallback)

This means query parameters take precedence over headers.

## Key Concepts

### When to Use ProfileName vs ProfileNameUnverified

**Use `ProfileName` when:**

- You need to ensure the profile exists before processing
- Performing operations on existing profiles
- Building secure endpoints that require registered users

**Use `ProfileNameUnverified` when:**

- Registering new profiles
- You'll verify the profile manually later
- Building public endpoints that accept any profile identifier

### Error Handling

Both extractors return `400 Bad Request` with descriptive messages:

- `ProfileName`: "Missing moosicbox-profile header" or "Profile 'X' does not exist"
- `ProfileNameUnverified`: "Missing moosicbox-profile header"

These errors are automatically handled by actix-web and returned to the client.

### Thread Safety

All profile operations are thread-safe. The actix-web server can handle concurrent requests, and the `PROFILES` registry safely manages concurrent access using `RwLock`.

## Testing the Example

### 1. List All Profiles

```bash
curl http://127.0.0.1:8080/profiles
```

Expected response:

```json
{
    "status": "success",
    "count": 3,
    "profiles": ["admin", "alice", "bob"]
}
```

### 2. Request with Verified Profile (Header)

```bash
curl -H "moosicbox-profile: alice" http://127.0.0.1:8080/verified
```

Expected response:

```json
{
    "status": "success",
    "profile": "alice",
    "verified": true,
    "message": "Welcome, alice!"
}
```

### 3. Request with Verified Profile (Query Param)

```bash
curl "http://127.0.0.1:8080/verified?moosicboxProfile=bob"
```

Expected response:

```json
{
    "status": "success",
    "profile": "bob",
    "verified": true,
    "message": "Welcome, bob!"
}
```

### 4. Request with Non-Existent Profile

```bash
curl -H "moosicbox-profile: unknown" http://127.0.0.1:8080/verified
```

Expected response (400 Bad Request):

```
Missing moosicboxProfile query param
```

### 5. Unverified Profile Request

```bash
curl "http://127.0.0.1:8080/unverified?moosicboxProfile=newuser"
```

Expected response:

```json
{
    "status": "success",
    "profile": "newuser",
    "verified": false,
    "message": "Processing request for profile: newuser"
}
```

### 6. Register New Profile

```bash
curl -X POST -H "moosicbox-profile: charlie" http://127.0.0.1:8080/register
```

Expected response:

```json
{
    "status": "success",
    "message": "Profile registered successfully",
    "profile": "charlie"
}
```

Now verify it's registered:

```bash
curl -H "moosicbox-profile: charlie" http://127.0.0.1:8080/verified
```

### 7. Missing Profile Information

```bash
curl http://127.0.0.1:8080/verified
```

Expected response (400 Bad Request):

```
Missing moosicbox-profile header
```

### 8. Query Parameter Takes Precedence

```bash
curl -H "moosicbox-profile: alice" "http://127.0.0.1:8080/verified?moosicboxProfile=bob"
```

Response will use `bob` (from query param), not `alice` (from header).

## Troubleshooting

### Server won't start - "Address already in use"

Another process is using port 8080. Either kill that process or modify the `bind()` call in `main.rs` to use a different port:

```rust
.bind(("127.0.0.1", 8081))?
```

### Getting 400 errors with valid profiles

Ensure the profile exists in the registry. Use the `/profiles` endpoint to list all registered profiles, or use the `/register` endpoint to add new ones.

### Profile extraction not working

Check that you're providing the profile information correctly:

- Header name: `moosicbox-profile` (lowercase, with hyphen)
- Query param: `moosicboxProfile` (camelCase)
- Both are case-sensitive

## Related Examples

- `basic_usage` - Demonstrates core profile registry operations
- `events` - Shows how to subscribe to profile update events
