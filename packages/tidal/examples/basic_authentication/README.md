# Basic Authentication Example

A comprehensive example demonstrating OAuth2 device authorization flow and basic API usage with the Tidal music streaming service.

## Summary

This example walks through the complete authentication workflow for Tidal, including device authorization, token exchange, and retrieving user favorites (artists and albums) using the `TidalMusicApi` client.

## What This Example Demonstrates

- **OAuth2 Device Authorization Flow**: Complete workflow for authenticating with Tidal using device codes
- **Token Management**: Exchanging device codes for access tokens and persisting credentials
- **API Client Initialization**: Creating and configuring a `TidalMusicApi` instance
- **Favorites Retrieval**: Fetching favorite artists and albums with pagination
- **Search Functionality**: Searching the Tidal catalog for artists, albums, and tracks
- **Error Handling**: Proper use of Result types and error propagation
- **Interactive User Input**: Collecting credentials and waiting for user actions

## Prerequisites

Before running this example, you need:

- **Tidal Developer Credentials**:
    - A Tidal Client ID
    - A Tidal Client Secret
    - These can be obtained by registering an application with Tidal's developer program
- **Tidal Account**: A valid Tidal account to authorize with
- **Internet Connection**: Required for API communication
- **Web Browser**: For completing the OAuth authorization flow

## Running the Example

From the repository root, run:

```bash
cargo run --manifest-path packages/tidal/examples/basic_authentication/Cargo.toml
```

### Interactive Flow

1. The example will prompt you to enter your Client ID and Client Secret
2. A browser window will open to Tidal's authorization page
3. Log in to your Tidal account and authorize the application
4. Return to the terminal and press Enter to continue
5. The example will exchange the device code for an access token
6. It will then fetch and display your favorite artists and albums
7. Finally, it will perform a search for "Pink Floyd" to demonstrate search functionality

## Expected Output

```
=== MoosicBox Tidal Authentication Example ===

Enter your Tidal Client ID: [your-client-id]
Enter your Tidal Client Secret: [your-client-secret]

--- Starting OAuth2 Device Authorization Flow ---

Device authorization initiated!
Device Code: XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX
User Code: XXXXXX
Verification URI: https://link.tidal.com/XXXXX
Complete Verification URI: https://link.tidal.com/XXXXX?userCode=XXXXXX
Authorization expires in 300 seconds

The authorization page should have opened in your browser.
Please complete the authorization process...

Press Enter after completing authorization in your browser...

--- Exchanging Device Code for Access Token ---

✓ Authentication successful!
Access token obtained: eyJhbGciOiJIUzI1NiIs...
Token expires in: 86400 seconds
Token type: Bearer

--- Initializing Tidal API Client ---

✓ Tidal API client initialized

--- Fetching Favorite Artists ---

✓ Retrieved 5 favorite artists
Total favorite artists: 27
  1. Pink Floyd (ID: 3248)
     Cover: https://resources.tidal.com/images/...
  2. Led Zeppelin (ID: 179)
     Cover: https://resources.tidal.com/images/...
  [...]

--- Fetching Favorite Albums ---

✓ Retrieved 10 favorite albums
Total favorite albums: 143
  1. The Dark Side of the Moon (ID: 12345)
     Artist: Pink Floyd
     Year: 1973
     Artwork: https://resources.tidal.com/images/...
  [...]

--- Searching for 'Pink Floyd' ---

✓ Search completed

Artist Results (1):
  1. Pink Floyd

Album Results (5):
  1. The Dark Side of the Moon - Pink Floyd
  2. The Wall - Pink Floyd
  [...]

Track Results (5):
  1. Comfortably Numb - Pink Floyd
  2. Time - Pink Floyd
  [...]

=== Example Completed Successfully ===

Note: In this example, we used an in-memory database.
In production, use a persistent database to store credentials across runs.
```

## Code Walkthrough

### 1. OAuth2 Device Authorization

The example starts by initiating the device authorization flow:

```rust
let auth_response = device_authorization(client_id.clone(), true).await?;
```

This returns a `device_code` and `user_code`, and automatically opens the authorization URL in the user's browser. The user must complete authorization on the Tidal website.

### 2. Token Exchange

After the user authorizes, exchange the device code for an access token:

```rust
let token_response = device_authorization_token(
    &db,
    client_id.clone(),
    client_secret.clone(),
    auth_response.device_code.clone(),
    Some(true), // persist to database
).await?;
```

Setting `persist_to_db` to `true` stores the credentials in the database for future use.

### 3. API Client Initialization

Create a `TidalMusicApi` instance using the builder pattern:

```rust
let tidal_api = TidalMusicApi::builder()
    .with_db(db)
    .with_client_id(client_id)
    .with_client_secret(client_secret)
    .build()
    .await?;
```

The API client automatically retrieves stored credentials from the database.

### 4. Fetching Favorites

Retrieve favorite artists using the `MusicApi` trait:

```rust
let artists = tidal_api
    .artists(
        Some(0),  // offset
        Some(10), // limit
        None,     // order
        None,     // order direction
    )
    .await?;
```

Similarly, fetch favorite albums:

```rust
let albums_request = AlbumsRequest {
    page: Some(PagingRequest {
        offset: 0,
        limit: 10,
    }),
    sort: None,
    filters: None,
};

let albums = tidal_api.albums(&albums_request).await?;
```

### 5. Search Functionality

Search the Tidal catalog:

```rust
let search_results = tidal_api
    .search(
        "Pink Floyd",
        Some(0),
        Some(5),
    )
    .await?;
```

The search returns results for artists, albums, and tracks in separate collections.

## Key Concepts

### OAuth2 Device Authorization Flow

The device authorization flow is designed for devices with limited input capabilities (like TVs or IoT devices), but it's also convenient for CLI applications:

1. **Device Authorization**: Request a device code and user code
2. **User Authorization**: User visits a URL and enters the user code (or clicks a complete verification URI)
3. **Token Exchange**: Exchange the device code for an access token
4. **Token Persistence**: Store the access token for future API requests

### Database Persistence

The example uses an in-memory database (`LibraryDatabase::new_in_memory()`), which means credentials are lost when the application exits. In production:

```rust
// Use a persistent database
let db = LibraryDatabase::new("path/to/database.db").await?;
```

This allows the application to reuse stored credentials without re-authenticating.

### MusicApi Trait

`TidalMusicApi` implements the `MusicApi` trait, providing a consistent interface for music streaming services. This allows the same code to work with different streaming providers (Tidal, Qobuz, etc.) by simply swapping the implementation.

### Pagination

All list endpoints (artists, albums, tracks) support pagination:

- **offset**: Starting position (0-indexed)
- **limit**: Maximum number of items to return

This is crucial for efficiently handling large collections without loading everything into memory.

## Testing the Example

### With Real Credentials

1. Obtain Tidal developer credentials from https://developer.tidal.com/
2. Run the example and enter your credentials
3. Authorize the application in your browser
4. Verify the output shows your actual favorite artists and albums

### Without Real Credentials

If you don't have Tidal developer credentials, you can still review the code to understand:

- The OAuth2 device flow pattern
- How to structure API client initialization
- Pagination and result handling
- Error handling with Result types

## Troubleshooting

### "Authorization expired"

**Problem**: The device code expired before you completed authorization.

**Solution**: Device codes typically expire in 5 minutes. Run the example again and complete authorization promptly.

### "Unauthorized" error

**Problem**: The access token is invalid or expired.

**Solution**: Re-run the example to obtain a fresh token. In production, implement token refresh logic.

### Browser doesn't open

**Problem**: The authorization URL didn't open automatically.

**Solution**: Manually copy and paste the "Verification URI" shown in the output into your browser.

### No favorites returned

**Problem**: The example shows 0 favorite artists/albums.

**Solution**: This is expected if your Tidal account has no favorites. Try adding some favorites in the Tidal app first.

### Database errors

**Problem**: Errors related to database operations.

**Solution**: Ensure the `db` feature is enabled (it is by default in this example). Check file permissions if using a persistent database.

## Related Examples

- **moosicbox_qobuz examples**: Similar streaming service integration with different authentication flow
- **moosicbox_music_api examples**: Core music API trait usage patterns
- **moosicbox_player examples**: Using streaming APIs with audio playback

---

**Note**: This example uses real Tidal API credentials and makes actual API calls. Ensure you comply with Tidal's API terms of service and rate limits when building production applications.
