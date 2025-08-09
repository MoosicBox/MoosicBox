# MoosicBox Determinism Audit

## Executive Summary

This document audits non-deterministic patterns in the MoosicBox codebase, analyzing their scope and complexity. Each section describes the extent of the issue and what would be required to fix it.

**Scope of Issues (by size):**

- **Largest:** Direct actix-web usage in 50+ packages (requires creating abstractions and migrating all web endpoints)
- **Medium:** Missing switchy packages (uuid, env, process) and adoption of existing ones (fs, tcp, http)
- **Smallest:** Mechanical replacements (HashMap→BTreeMap, adding sort to directory operations)

The **Optimized Execution Plan** section provides the recommended order for addressing these issues, which prioritizes quick wins over tackling the largest problems first.

## Status Legend

- 🔴 **Critical** - Affects core functionality or security
- 🟡 **Important** - Affects reliability or testing
- 🟢 **Minor** - Cosmetic or non-critical
- ✅ **Fixed** - Already addressed
- ⏳ **In Progress** - Currently being worked on
- ❌ **Blocked** - Waiting on dependencies or design decisions

## 1. Web Server Framework (Actix-Web)

**Status:** 🔴 CRITICAL | ❌ Major refactoring required

### Scope of Problem

- **50+ packages** directly using `actix_web` instead of `moosicbox_web_server`
- **275+ references** to `actix_web::` throughout codebase
- **173+ uses** of actix extractors (`web::Json`, `web::Query`, `web::Path`, `web::Data`)
- **17 instances** of `HttpServer::new` or `App::new`

### Major Affected Packages

| Package                   | Usage                    | Complexity |
| ------------------------- | ------------------------ | ---------- |
| `packages/admin_htmx/`    | Full API implementation  | 🔴 High    |
| `packages/auth/`          | Authentication endpoints | 🔴 High    |
| `packages/config/`        | Configuration API        | 🔴 High    |
| `packages/downloader/`    | Download API             | 🟡 Medium  |
| `packages/files/`         | File serving API         | 🔴 High    |
| `packages/library/`       | Library API              | 🔴 High    |
| `packages/menu/`          | Menu API                 | 🟡 Medium  |
| `packages/music_api/`     | Music API endpoints      | 🔴 High    |
| `packages/player/`        | Player control API       | 🔴 High    |
| `packages/scan/`          | Scan API                 | 🟡 Medium  |
| `packages/search/`        | Search API               | 🟡 Medium  |
| `packages/server/`        | Main server              | 🔴 High    |
| `packages/tunnel_server/` | Tunnel server            | 🔴 High    |
| `packages/upnp/`          | UPnP API                 | 🟡 Medium  |
| `packages/audio_zone/`    | Audio zone API           | 🟡 Medium  |
| `packages/profiles/`      | Profiles API             | 🟡 Medium  |

### Required moosicbox_web_server Enhancements

The current `moosicbox_web_server` has some features but needs significant enhancements to achieve feature parity with actix-web:

#### Already Implemented ✅

- [x] **CORS support** - via `moosicbox_web_server_cors` package
- [x] **OpenAPI support** - via utoipa integration
- [x] **Basic routing** - Scope and route handlers
- [x] **Compression** - feature flag exists (needs testing)
- [x] **Request/Response abstractions** - HttpRequest, HttpResponse

#### Missing Core Features ❌

- [ ] **Extractors**: Json, Query, Path, Data, Form (heavily used in all API packages)
- [ ] **Middleware System**: General middleware trait/framework (only CORS exists)
- [ ] **WebSocket Support**: Full WS/WSS implementation (critical for server/tunnel_server)
- [ ] **Streaming**: Server-sent events, chunked responses
- [ ] **Static Files**: File serving with range requests
- [ ] **State Management**: App data injection (web::Data<T> pattern)
- [ ] **Guards**: Request guards and filters
- [ ] **Error Handling**: Rich error response system

#### Currently Used Actix Features Requiring Migration

- `web::Query<T>` - Used in 100+ endpoints
- `web::Json<T>` - Used for JSON request/response
- `web::Path<T>` - Used for URL path parameters
- `web::Data<T>` - Used for shared state/database connections
- Custom middleware (auth, telemetry, logging)
- WebSocket handlers in server and tunnel_server

### Web Server Migration Steps

The web server migration will be executed as part of Phases 3-4 of the main execution plan:

**During Phase 3 (Web Server Preparation):**

- Enhance moosicbox_web_server with missing features
- Create trait abstractions for web concepts
- Build compatibility layer

**During Phase 4 (Web Server Migration):**

- Migrate leaf packages first (lowest dependencies)
- Migrate package groups in parallel
- Migrate core server packages last
- Remove actix-web dependency

### Migration Complexity

- **Enhancement of moosicbox_web_server**: High complexity, many missing features
- **Package migration**: Mechanical but extensive changes across 50+ packages
- **Testing and validation**: Critical for ensuring no regressions
- **Dependencies**: Must complete in order, but package groups can migrate in parallel

## 2. UUID Generation

**Status:** ✅ Fixed

### Solution Implemented

Created `switchy_uuid` package with:

- **Production**: Uses cryptographically secure random UUIDs via `uuid` crate
- **Simulation**: Uses seeded deterministic UUIDs for reproducible testing
- **Environment Control**: Set `SIMULATOR_UUID_SEED` to control deterministic generation

### Migrated Files

- ✅ `packages/tunnel_server/src/api.rs:110,129` - Token generation
- ✅ `packages/auth/src/lib.rs:75,88` - Magic token & session IDs
- ✅ `packages/simvar/examples/api_testing/src/main.rs:276,398` - Test data

All UUID generation now uses `switchy_uuid::{new_v4, new_v4_string}` functions.

## 2. Chrono Date/Time Usage

**Status:** 🟡 Important | ❌ Blocked (needs switchy_chrono or extension)

### Occurrences

| File                                         | Line    | Usage                                      | Priority     |
| -------------------------------------------- | ------- | ------------------------------------------ | ------------ |
| `packages/yt/src/lib.rs`                     | 1814    | `chrono::Local::now()` for date formatting | 🟡 Important |
| `packages/database/src/postgres/postgres.rs` | 1601    | `Utc::now()` for timestamps                | 🟡 Important |
| `packages/json_utils/src/database.rs`        | 282-569 | Chrono type serialization                  | 🟢 Minor     |

### Recommendation

Extend `switchy_time` to include:

- `datetime_now()` returning chrono DateTime types
- Timezone-aware time functions
- Date formatting utilities

## 3. Non-Deterministic Collections

**Status:** 🟢 Minor | ✅ Mostly Complete (83% done)

### Major Offenders (100+ occurrences)

| Package                   | Files                             | Usage                 | Priority     |
| ------------------------- | --------------------------------- | --------------------- | ------------ |
| `packages/ws/`            | `ws.rs`, `server.rs`              | Connection management | 🔴 Critical  |
| `packages/server/`        | `ws/server.rs`, `players/upnp.rs` | State management      | 🔴 Critical  |
| `packages/hyperchad/`     | Multiple renderer files           | UI state              | 🟡 Important |
| `packages/scan/`          | `output.rs`                       | Database ID tracking  | 🟡 Important |
| `packages/tunnel_sender/` | `sender.rs`                       | Abort token storage   | 🟡 Important |
| `packages/upnp/`          | `listener.rs`                     | Status handles        | 🟡 Important |

### Fixed ✅

- `packages/async/src/simulator/runtime.rs`
- `packages/server/src/players/local.rs`
- `packages/tunnel_server/src/auth.rs`, `api.rs`
- `packages/qobuz/src/lib.rs`
- `packages/library/src/cache.rs`
- `packages/database_connection/src/creds.rs`

### Recommendation

Systematic replacement of all HashMap/HashSet with BTreeMap/BTreeSet

## 4. Thread/Task Spawning

**Status:** 🟡 Important | ❌ Needs design

### Occurrences (32 instances)

| Package                         | Pattern              | Usage              | Priority     |
| ------------------------------- | -------------------- | ------------------ | ------------ |
| `packages/simvar/harness/`      | `std::thread::spawn` | Test harness       | 🟢 Minor     |
| `packages/tcp/src/simulator.rs` | `task::spawn`        | Network simulation | 🟡 Important |
| `packages/audio_output/`        | `thread::spawn`      | Resource daemon    | 🟡 Important |
| `packages/app/native/`          | `task::spawn`        | UI tasks           | 🟡 Important |

### Recommendation

- Create deterministic task scheduler for simulations
- Use ordered task queues where execution order matters
- Document where non-deterministic spawning is acceptable

## 5. Random Number Generation

**Status:** ✅ Fixed

### Solution Implemented

All random operations now use `switchy_random` package which provides:

- Deterministic random for testing (via simulator feature)
- Real random for production (via rand feature)

### Previously Fixed

- `packages/clippier/src/lib.rs` - Now uses `switchy_random`
- `packages/openport/src/lib.rs` - Now uses `switchy_random`
- All other random usage migrated to `switchy_random`

No further action needed.

## 6. Time Operations

**Status:** 🟡 Important | ⏳ Mostly Fixed

### Solution

`switchy_time` package provides deterministic time with:

- `now()` for SystemTime
- `instant_now()` for Instant (recently added)
- Simulator and standard implementations

### Fixed ✅

- WebSocket heartbeats now use `switchy_time::instant_now()`
- Performance measurements use `switchy_time::instant_now()`
- Timestamps use `switchy_time::now()`

### Remaining Direct Usage

| File                                | Line     | Usage              | Priority     |
| ----------------------------------- | -------- | ------------------ | ------------ |
| `packages/files/src/lib.rs`         | 161, 192 | Performance timing | 🟢 Minor     |
| `packages/audio_output/src/cpal.rs` | 596      | Audio timing       | 🟡 Important |

These should migrate to use `switchy_time`.

## 7. Environment Variables

**Status:** ✅ Fixed (Core Infrastructure) | ✅ High Priority Complete | ✅ Medium Priority Complete | ✅ Low Priority Complete | ✅ MIGRATION COMPLETE

### Solution Implemented

Created `switchy_env` package with:

- **Production**: Uses real environment variables via `std::env`
- **Simulation**: Uses configurable environment with deterministic defaults
- **Type Safety**: Parse environment variables to specific types with `var_parse<T>()`
- **Optional Parsing**: New `var_parse_opt<T>() -> Result<Option<T>, EnvError>` for better error handling
- **Backward Compatibility**: Support both "1" and "true" for boolean flags using `matches!` pattern
- **Testing**: Set/remove variables for testing scenarios

### 🚫 **DO NOT MIGRATE** - Must Use Real Environment Variables (30+ locations)

These packages control simulation behavior and build processes - they MUST use real environment variables:

#### **Switchy Infrastructure Packages** (Control simulation behavior)

```
❌ packages/random/src/simulator.rs:13,48 - SIMULATOR_SEED
❌ packages/time/src/simulator.rs:26,63 - SIMULATOR_EPOCH_OFFSET, SIMULATOR_STEP_MULTIPLIER
❌ packages/simvar/harness/src/lib.rs:52,115 - SIMULATOR_RUNS, SIMULATOR_MAX_PARALLEL
❌ packages/simvar/harness/src/config.rs:55,377 - SIMULATOR_DURATION, std::env::vars()
```

#### **Compile-Time Environment Variables** (Build-time constants)

```
❌ packages/tunnel_server/src/auth.rs:16 - env!("TUNNEL_ACCESS_TOKEN")
❌ packages/server/src/api/mod.rs:19 - env!("GIT_HASH")
❌ packages/server/src/lib.rs:315 - env!("STATIC_TOKEN")
❌ packages/telemetry/src/lib.rs:54 - env!("CARGO_PKG_VERSION")
❌ All env!() macro usage (8+ locations)
```

#### **Build Scripts & Macros** (Build environment)

```
❌ packages/async/macros/src/lib.rs:299 - CARGO_MANIFEST_DIR (macro expansion)
❌ packages/hyperchad/app/src/renderer.rs:298 - CARGO_MANIFEST_DIR (asset resolution)
❌ packages/hyperchad/renderer/vanilla_js/build.rs:15 - CARGO_MANIFEST_DIR
❌ All CARGO_MANIFEST_DIR usage (10+ locations)
```

#### **Switchy_env Package Itself** (The abstraction layer)

```
❌ packages/env/src/simulator.rs:16,68 - std::env::vars() (loading real env vars)
❌ packages/env/src/standard.rs:22,26 - std::env::var(), std::env::vars()
```

### ✅ **SHOULD MIGRATE** - Application Logic Environment Variables (48+ locations)

#### **1. Database Configuration** (🔴 High Priority) ✅ COMPLETED

```
✅ packages/database_connection/src/creds.rs:
   - Line 38: DATABASE_URL ✅ MIGRATED
   - Lines 44-47: DB_HOST, DB_NAME, DB_USER, DB_PASSWORD ✅ MIGRATED
   - Lines 72-78: SSM_DB_NAME_PARAM_NAME, SSM_DB_HOST_PARAM_NAME,
                  SSM_DB_USER_PARAM_NAME, SSM_DB_PASSWORD_PARAM_NAME ✅ MIGRATED

✅ packages/schema/src/lib.rs:236 - MOOSICBOX_SKIP_MIGRATION_EXECUTION ✅ MIGRATED
```

#### **2. Authentication & Security** (🔴 High Priority) ✅ COMPLETED

```
✅ packages/auth/src/lib.rs:120 - TUNNEL_ACCESS_TOKEN (runtime token) ✅ MIGRATED
✅ packages/app/native/ui/src/api/tidal.rs:16,65-66 - TIDAL_CLIENT_ID, TIDAL_CLIENT_SECRET ✅ MIGRATED
```

#### **3. Service Configuration** (🔴 High Priority) ✅ COMPLETED

```
✅ packages/load_balancer/src/load_balancer.rs:
   - Line 12: PORT ✅ MIGRATED (using var_parse_or)
   - Line 19: SSL_PORT ✅ MIGRATED (using var_parse_or)
   - Line 26: SSL_CRT_PATH ✅ MIGRATED (using var_or)
   - Line 30: SSL_KEY_PATH ✅ MIGRATED (using var_or)

✅ packages/load_balancer/src/server.rs:44,81 - CLUSTERS, SSL path checks ✅ MIGRATED
✅ packages/server/simulator/src/main.rs:11 - PORT ✅ MIGRATED (using var_parse_opt)
✅ packages/upnp/src/player.rs:382 - UPNP_SEND_SIZE ✅ MIGRATED (using var_parse_or<bool>)
```

#### **4. Telemetry & Monitoring** (🟡 Medium Priority) ✅ COMPLETED

```
✅ packages/telemetry/src/lib.rs:44 - OTEL_ENDPOINT ✅ MIGRATED (using var_or)
```

#### **5. Debug & Development Flags** (🟢 Low Priority) ✅ COMPLETED

```
✅ packages/app/tauri/src-tauri/src/lib.rs:677 - TOKIO_CONSOLE ✅ MIGRATED (supports "1" and "true")
✅ packages/app/native/src/main.rs:29 - TOKIO_CONSOLE ✅ MIGRATED (supports "1" and "true")
✅ packages/marketing_site/src/main.rs:24 - TOKIO_CONSOLE ✅ MIGRATED (supports "1" and "true")
✅ packages/tunnel_server/src/main.rs:49 - TOKIO_CONSOLE ✅ MIGRATED (supports "1" and "true")
✅ packages/server/src/main.rs:38 - TOKIO_CONSOLE ✅ MIGRATED (supports "1" and "true")
✅ packages/hyperchad/renderer/egui/src/v1.rs:38 - DEBUG_RENDERER ✅ MIGRATED (supports "1" and "true")
✅ packages/hyperchad/renderer/fltk/src/lib.rs:56 - DEBUG_RENDERER ✅ MIGRATED (supports "1" and "true")
```

#### **6. Environment Utilities Package Status** ✅ COMPLETED

```
✅ packages/env_utils/src/lib.rs - Runtime environment utilities REMOVED
   Status: COMPILE-TIME ONLY PACKAGE - Runtime functions successfully removed
   Contains: Compile-time macros only (preserved) - 15+ macros using env!() and option_env!()
```

**✅ Runtime Functions Removal Complete**: All 15 runtime functions that used `std::env::var()` have been successfully removed from `moosicbox_env_utils` and replaced with `switchy_env` calls across 13+ packages.

**Functions Removed**: `env_usize()`, `default_env_usize()`, `default_env_u16()`, `option_env_usize()`, `option_env_u64()`, `option_env_u32()`, `option_env_u16()`, `option_env_f32()`, `option_env_isize()`, `option_env_i64()`, `option_env_i32()`, `option_env_i16()`, `option_env_i8()`, `default_env()`, and 4 error types.

**Macros Preserved**: All compile-time macros using `env!()` and `option_env!()` remain for build-time constants: `env_usize!`, `default_env!`, `default_env_usize!`, `option_env_usize!`, etc.

**Packages Migrated**: marketing_site, tunnel_server, server, hyperchad/app, app/native, app/tauri, hyperchad/renderer/html/web_server, logging, and 5+ others.

### Migration Status Summary

**Completed Core Infrastructure:**

- ✅ Correctly preserved simulation control variables (SIMULATOR\_\*)
- ✅ Correctly preserved compile-time constants (env!() macros)
- ✅ Correctly preserved build environment (CARGO_MANIFEST_DIR)

**✅ COMPLETED High Priority Application Migration:**

- ✅ **Critical (18 locations)**: Database credentials, authentication tokens, service configuration - ALL MIGRATED
    - Database connection credentials (6 variables)
    - Authentication tokens (TUNNEL_ACCESS_TOKEN)
    - TIDAL API credentials (CLIENT_ID, CLIENT_SECRET)
    - Load balancer configuration (PORT, SSL_PORT, SSL paths, CLUSTERS)
    - Schema migration flag (MOOSICBOX_SKIP_MIGRATION_EXECUTION)

**✅ COMPLETED Medium Priority Application Migration:**

- ✅ **Important (9 locations)**: Telemetry, UPnP settings, server simulator - ALL MIGRATED
    - Telemetry endpoint configuration (OTEL_ENDPOINT)
    - UPnP send size flag (UPNP_SEND_SIZE)
    - Server simulator port (PORT with proper error handling)

**✅ COMPLETED Low Priority Application Migration:**

- ✅ **Debug flags (7+ locations)**: Console debugging, renderer debugging - ALL MIGRATED
    - TOKIO_CONSOLE debug flags (5 packages) - supports both "1" and "true"
    - DEBUG_RENDERER flags (2 packages) - supports both "1" and "true"

#### **7. Additional Debug & Development Variables** (🟢 Low Priority) ✅ COMPLETED

```
✅ packages/hyperchad/transformer/src/lib.rs:
   - Line 2826: SKIP_DEFAULT_DEBUG_ATTRS ✅ MIGRATED (supports "1" and "true")
   - Line 3424: DEBUG_ATTRS ✅ MIGRATED (supports "1" and "true")
   - Line 3430: DEBUG_RAW_ATTRS ✅ MIGRATED (supports "1" and "true")

✅ packages/hyperchad/js_bundler/src/node.rs:36 - PNPM_HOME ✅ MIGRATED (build tool detection)
```

#### **8. Runtime Functions Removal** (🟢 Cleanup) ✅ COMPLETED

```
✅ moosicbox_env_utils runtime functions removal:
   - 15 runtime functions removed: env_usize(), default_env_usize(), default_env_u16(),
     option_env_usize(), option_env_u64(), option_env_u32(), option_env_u16(),
     option_env_f32(), option_env_isize(), option_env_i64(), option_env_i32(),
     option_env_i16(), option_env_i8(), default_env()
   - 4 error types removed: EnvUsizeError, DefaultEnvUsizeError, OptionEnvUsizeError, OptionEnvF32Error
   - 15+ compile-time macros preserved: env_usize!, default_env!, option_env_*!, etc.

✅ 13+ packages migrated to switchy_env:
   - packages/marketing_site/ (3 files) - default_env_usize(), option_env_f32(), option_env_i32()
   - packages/tunnel_server/ (1 file) - default_env(), default_env_usize(), option_env_usize()
   - packages/server/ (2 files) - default_env(), default_env_usize(), option_env_usize()
   - packages/hyperchad/app/ (1 file) - default_env_usize()
   - packages/app/native/ (1 file) - default_env_usize(), option_env_f32(), option_env_i32()
   - packages/app/tauri/src-tauri/ (1 file) - default_env_u16()
   - packages/hyperchad/renderer/html/web_server/ (1 file) - default_env()
   - packages/logging/ (1 file) - unused import cleanup
   - 5+ additional packages with switchy_env dependencies added
```

**Migration Pattern Used:**

```rust
// Before: Runtime function calls
let threads = default_env_usize("MAX_THREADS", 64).unwrap_or(64);
let port = default_env("PORT", "8080");

// After: switchy_env calls
let threads = var_parse_or("MAX_THREADS", 64usize);
let port = var_or("PORT", "8080");
```

**🎉 MIGRATION 100% COMPLETE + RUNTIME FUNCTIONS REMOVED:**

- **Total migrated**: 38+ environment variables across 17+ packages
- **All priority levels**: High, Medium, and Low priority migrations completed
- **Additional variables**: 4 debug/development variables migrated
- **Runtime functions removed**: 15 functions + 4 error types from moosicbox_env_utils
- **Packages migrated from runtime functions**: 13+ packages migrated to switchy_env
- **Backward compatibility**: Maintained for all existing usage patterns
- **Enhanced API**: New `var_parse_opt` function for better error handling
- **Compile-time macros**: All 15+ macros preserved for build-time constants

### Usage Pattern

```rust
use switchy_env::{var, var_or, var_parse, var_parse_or, var_parse_opt};

// Database configuration with deterministic defaults
let database_url = var_or("DATABASE_URL", "sqlite::memory:");
let db_host = var_or("DB_HOST", "localhost");

// Service configuration with type safety
let port: u16 = var_parse_or("PORT", 8080);
let ssl_port: u16 = var_parse_or("SSL_PORT", 8443);

// Optional configuration with proper error handling
let optional_port: Option<u16> = var_parse_opt("OPTIONAL_PORT")
    .expect("Invalid OPTIONAL_PORT env var")?;

// Boolean flags supporting both "1" and "true" (backward compatibility)
let tokio_console = matches!(var("TOKIO_CONSOLE").as_deref(), Ok("1") | Ok("true"));
let debug_renderer = matches!(var("DEBUG_RENDERER").as_deref(), Ok("1") | Ok("true"));

// Authentication tokens (no defaults for security)
let tunnel_token = var("TUNNEL_ACCESS_TOKEN")?;
```

## 8. File System Operations

**Status:** 🟡 Important | ⏳ Partial solution exists

### Problem

Many packages directly use `std::fs` instead of `switchy_fs`, and don't sort directory listings for deterministic ordering.

### Major Areas Not Using switchy_fs

| Package                | Operation      | Usage           | Priority     |
| ---------------------- | -------------- | --------------- | ------------ |
| `packages/scan/`       | `fs::read_dir` | Music scanning  | 🔴 Critical  |
| `packages/files/`      | Directory ops  | File management | 🟡 Important |
| `packages/downloader/` | File writing   | Downloads       | 🟡 Important |
| `packages/clippier/`   | File I/O       | Build tools     | 🟢 Minor     |

### Recommendation

- Migrate all file operations to use existing `switchy_fs` package
- Always sort directory listings before processing (add `.sort()` after collecting entries)
- Use `switchy_fs::simulator` for testing

## 9. Process/Command Execution

**Status:** 🟡 Important | ❌ No abstraction exists

### Problem

Direct use of `std::process::Command` without abstraction layer. Need to create `switchy_process` package.

### Direct Usage Occurrences (29 instances)

| File                             | Command         | Usage          | Priority     |
| -------------------------------- | --------------- | -------------- | ------------ |
| `packages/bloaty/src/main.rs`    | `cargo`         | Build analysis | 🟢 Minor     |
| `packages/server/src/lib.rs:769` | `puffin_viewer` | Profiling      | 🟢 Minor     |
| `build.rs` files                 | `git`           | Version info   | 🟢 Minor     |
| `packages/assert/src/lib.rs`     | `process::exit` | Error handling | 🟡 Important |

### Recommendation

Create new `switchy_process` package with:

- Command execution abstraction
- Deterministic output for testing
- Process exit handling

## 10. Network Operations

**Status:** 🔴 Critical | ⏳ Abstractions exist but underutilized

### Problem

Many packages still use direct network operations instead of existing `switchy_tcp` and `switchy_http` abstractions.

### Packages Not Using Switchy Network Abstractions

| Package                   | Current Usage   | Should Use                    | Priority     |
| ------------------------- | --------------- | ----------------------------- | ------------ |
| `packages/tunnel_sender/` | Direct TCP/HTTP | `switchy_tcp`, `switchy_http` | 🔴 Critical  |
| `packages/upnp/`          | Direct sockets  | `switchy_tcp`                 | 🟡 Important |
| `packages/openport/`      | Direct binding  | `switchy_tcp`                 | 🟡 Important |
| Various API packages      | Direct reqwest  | `switchy_http`                | 🔴 Critical  |

Note: `packages/tcp/` and `packages/http/` ARE the switchy abstractions - they don't need fixing.

### Recommendation

- Migrate all TCP operations to use `switchy_tcp`
- Migrate all HTTP operations to use `switchy_http`
- Use simulator features for deterministic testing

## 11. Async Race Conditions in Application Code

**Status:** 🔴 Critical | ⏳ Partial solution via switchy_async

### Problem

Application code has race conditions. `switchy_async` provides deterministic runtime for testing, but code needs to use it properly.

### Problem Areas in Application Code

| Pattern                     | Count | Risk                         | Priority     |
| --------------------------- | ----- | ---------------------------- | ------------ |
| `.await.unwrap()`           | 100+  | Panic on error               | 🟡 Important |
| `join_all` without ordering | 15+   | Non-deterministic completion | 🔴 Critical  |
| `select()` in handlers      | 10+   | Race conditions              | 🔴 Critical  |
| Concurrent DB ops           | 20+   | Data races                   | 🔴 Critical  |

### Recommendation

- Use `switchy_async` runtime for deterministic testing
- Replace `join_all` with sequential execution where order matters
- Add explicit synchronization points
- Document where race conditions are acceptable

## 12. Floating Point Operations

**Status:** 🟢 Minor | ⏳ Low priority

### Major Uses (100+ occurrences)

- Audio processing (acceptable non-determinism)
- UI positioning (acceptable for display)
- Progress calculations (should be deterministic)

### Recommendation

- Use fixed-point arithmetic for critical calculations
- Document acceptable floating-point usage
- Consider `ordered-float` for deterministic comparisons

## 13. Lock Ordering Issues

**Status:** 🔴 Critical | ❌ Needs systematic review

## 14. UI Framework Limitations (egui)

**Status:** 🟡 Important | ❌ Blocked by external dependency

### Problem

The egui UI framework requires HashMap for performance-critical operations. Converting these to BTreeMap would cause significant performance degradation in the UI.

### Affected Files

- `packages/hyperchad/renderer/egui/src/v1.rs:229-777` - UI state maps (15+ occurrences)
- `packages/hyperchad/renderer/egui/src/v2.rs:178-180,507` - UI element maps

### Recommendation

- Accept non-determinism in UI components as acceptable trade-off
- Document that UI state is intentionally non-deterministic
- Consider UI testing strategies that don't rely on deterministic state
- Focus determinism efforts on core business logic instead

### Risk Areas

| Package                     | Locks                 | Risk               | Priority     |
| --------------------------- | --------------------- | ------------------ | ------------ |
| `packages/server/`          | Multiple global locks | Deadlock potential | 🔴 Critical  |
| `packages/upnp/`            | Device mapping locks  | Ordering issues    | 🟡 Important |
| `packages/player/`          | Playback state        | Contention         | 🟡 Important |
| `packages/library/cache.rs` | Cache locks           | Performance        | 🟢 Minor     |

### Recommendation

- Establish global lock ordering hierarchy
- Use lock-free data structures where possible
- Add deadlock detection in debug builds

## Optimized Execution Plan

### Phase 1: Foundation & Quick Wins

**Goal: Maximum determinism improvement with minimal effort**

**Status: ✅ 100% Complete - HashMap/HashSet migration finished!**

**Parallel execution possible:**

#### 1.1 Replace ALL remaining HashMap/HashSet with BTreeMap/BTreeSet

**Progress: 28/30 files completed (93%)**

**✅ Completed Files (28/30):**

- [x] `packages/scan/src/output.rs:583,620,664` - HashSet<u64> for IDs
- [x] `packages/server/src/ws/server.rs:132,137,141` - Connection maps
- [x] `packages/server/src/auth.rs:108` - Query parameter collection
- [x] `packages/server/src/players/upnp.rs:23` - Player state map
- [x] `packages/ws/src/ws.rs:86` - CONNECTION_DATA static
- [x] `packages/player/src/api.rs:125` - PlaybackHandler map
- [x] `packages/player/src/lib.rs:228,364,365` - Query/headers maps
- [x] `packages/hyperchad/state/src/store.rs:14` - Cache storage
- [x] `packages/hyperchad/renderer/fltk/src/lib.rs:284` - Image cache
- [x] `packages/hyperchad/renderer/src/lib.rs:298,308` - Headers parameters
- [x] `packages/hyperchad/renderer/vanilla_js/src/lib.rs:798,815` - Headers
- [x] `packages/hyperchad/renderer/html/src/lib.rs:49,257,268` - Responsive triggers
- [x] `packages/hyperchad/renderer/html/src/actix.rs:267` - Static headers
- [x] `packages/hyperchad/renderer/html/src/html.rs:1046` - Headers
- [x] `packages/hyperchad/renderer/html/src/lambda.rs:253` - Lambda headers
- [x] `packages/hyperchad/renderer/html/src/web_server.rs:233` - Web server headers
- [x] `packages/hyperchad/renderer/html/http/src/lib.rs:95` - HTTP headers
- [x] `packages/hyperchad/actions/src/dsl.rs:448` - DSL variables
- [x] `packages/tunnel_server/src/ws/server.rs:332,343-352,530` - WebSocket state
- [x] `packages/tunnel_sender/src/sender.rs:187,275,619-1013` - Request tracking
- [x] `packages/tunnel/src/lib.rs:46` - Tunnel headers
- [x] `packages/files/src/files/track_pool.rs:85-86` - Semaphore/pool maps
- [x] `packages/upnp/src/listener.rs:68-69` - Status tracking
- [x] `packages/load_balancer/src/server.rs:27,43,66` - Cluster configuration
- [x] `packages/load_balancer/src/load_balancer.rs:35,39` - Router maps
- [x] `packages/app/tauri/src-tauri/src/lib.rs:1220,1270,1284` - Headers and state
- [x] `packages/app/native/src/visualization.rs:227` - Visualization cache
- [x] `packages/app/state/src/lib.rs:225,231,1165,1182,1200` - Audio zone and player state
- [x] `packages/clippier/src/common.rs:31` - HashSet for tracking changed packages
- [x] `packages/hyperchad/js_bundler/src/swc.rs:1,49` - HashMap for bundler entries
- [x] `packages/async_service/src/lib.rs:5` - Unused HashMap re-export (removed)

**❌ Blocked Files (2/30):**

- ❌ `packages/hyperchad/renderer/egui/src/v1.rs:229-777` - UI state maps (15+ occurrences) - **BLOCKED: egui requires HashMap for performance**
- ❌ `packages/hyperchad/renderer/egui/src/v2.rs:178-180,507` - UI element maps - **BLOCKED: egui requires HashMap for performance**

**⏳ Remaining Files (0/30):**

All HashMap/HashSet instances have been migrated to BTreeMap/BTreeSet or removed!

#### 1.2 Create `switchy_uuid` package ✅ COMPLETED

- [x] Create new package structure
- [x] Implement deterministic UUID generation for testing
- [x] Implement cryptographically secure UUIDs for production
- [x] Add seeded UUID generation for simulations

**Files migrated (6 direct usages):**

- [x] `packages/tunnel_server/src/api.rs:27,110,129` - Token generation
- [x] `packages/auth/src/lib.rs:16,75,88` - Magic token generation
- [x] `packages/simvar/examples/api_testing/src/main.rs:20,276,398` - Test IDs

#### 1.3 Create `switchy_env` package ✅ COMPLETED

- [x] Create new package structure
- [x] Implement environment variable abstraction
- [x] Add deterministic values for testing
- [x] Implement configuration injection
- [x] Add type-safe access patterns
- [x] Fixed Default trait conflict in standard implementation
- [x] Added proper feature flags (std, simulator)
- [x] Enhanced API with `var_parse_opt<T>() -> Result<Option<T>, EnvError>` for better error handling
- [x] Implemented backward compatibility pattern with `matches!` for "1"/"true" flags
- [x] Comprehensive testing and validation across all migrated packages

**✅ Correctly preserved simulation infrastructure (DO NOT MIGRATE):**

- ❌ `packages/random/src/simulator.rs:13,48` - SIMULATOR_SEED (controls simulation)
- ❌ `packages/time/src/simulator.rs:26,63` - SIMULATOR_EPOCH_OFFSET, SIMULATOR_STEP_MULTIPLIER (controls simulation)
- ❌ `packages/simvar/harness/src/lib.rs:52,115` - SIMULATOR_RUNS, SIMULATOR_MAX_PARALLEL (controls simulation)
- ❌ `packages/simvar/harness/src/config.rs:55,377` - SIMULATOR_DURATION (controls simulation)
- ❌ All compile-time env!() macros (8+ locations)
- ❌ All CARGO_MANIFEST_DIR usage (10+ locations)

**🔴 High Priority Application Migration (18 locations):**

- [x] `packages/database_connection/src/creds.rs:38-78` - Database credentials (10 env vars) ✅ COMPLETED
- [x] `packages/auth/src/lib.rs:120` - TUNNEL_ACCESS_TOKEN (runtime token) ✅ COMPLETED
- [x] `packages/app/native/ui/src/api/tidal.rs:16,65-66` - TIDAL_CLIENT_ID, TIDAL_CLIENT_SECRET ✅ COMPLETED
- [x] `packages/load_balancer/src/load_balancer.rs:12,19,26,30` - PORT, SSL_PORT, SSL paths ✅ COMPLETED
- [x] `packages/load_balancer/src/server.rs:44,81` - CLUSTERS, SSL configuration ✅ COMPLETED
- [x] `packages/schema/src/lib.rs:236` - MOOSICBOX_SKIP_MIGRATION_EXECUTION ✅ COMPLETED

**🟡 Medium Priority Application Migration (9 locations):** ✅ COMPLETED

- [x] `packages/server/simulator/src/main.rs:11` - PORT ✅ MIGRATED (using var_parse_opt with proper error handling)
- [x] `packages/upnp/src/player.rs:382` - UPNP_SEND_SIZE ✅ MIGRATED (supports "1" and "true")
- [x] `packages/telemetry/src/lib.rs:44` - OTEL_ENDPOINT ✅ MIGRATED (using var_or)

**🟢 Low Priority Application Migration (7+ locations):** ✅ COMPLETED

- [x] `packages/app/tauri/src-tauri/src/lib.rs:677` - TOKIO_CONSOLE ✅ MIGRATED (supports "1" and "true")
- [x] `packages/app/native/src/main.rs:29` - TOKIO_CONSOLE ✅ MIGRATED (supports "1" and "true")
- [x] `packages/marketing_site/src/main.rs:24` - TOKIO_CONSOLE ✅ MIGRATED (supports "1" and "true")
- [x] `packages/tunnel_server/src/main.rs:49` - TOKIO_CONSOLE ✅ MIGRATED (supports "1" and "true")
- [x] `packages/server/src/main.rs:38` - TOKIO_CONSOLE ✅ MIGRATED (supports "1" and "true")
- [x] `packages/hyperchad/renderer/egui/src/v1.rs:38` - DEBUG_RENDERER ✅ MIGRATED (supports "1" and "true")
- [x] `packages/hyperchad/renderer/fltk/src/lib.rs:56` - DEBUG_RENDERER ✅ MIGRATED (supports "1" and "true")

**🟢 Low Priority Debug Flags (7+ locations):** ✅ COMPLETED

- [x] `packages/app/tauri/src-tauri/src/lib.rs:677` - TOKIO_CONSOLE ✅ MIGRATED (supports "1" and "true")
- [x] `packages/app/native/src/main.rs:29` - TOKIO_CONSOLE ✅ MIGRATED (supports "1" and "true")
- [x] `packages/marketing_site/src/main.rs:24` - TOKIO_CONSOLE ✅ MIGRATED (supports "1" and "true")
- [x] `packages/tunnel_server/src/main.rs:49` - TOKIO_CONSOLE ✅ MIGRATED (supports "1" and "true")
- [x] `packages/server/src/main.rs:38` - TOKIO_CONSOLE ✅ MIGRATED (supports "1" and "true")
- [x] `packages/hyperchad/renderer/egui/src/v1.rs:38` - DEBUG_RENDERER ✅ MIGRATED (supports "1" and "true")
- [x] `packages/hyperchad/renderer/fltk/src/lib.rs:56` - DEBUG_RENDERER ✅ MIGRATED (supports "1" and "true")

**🟢 Additional Debug Variables (4 locations):** ✅ COMPLETED

- [x] `packages/hyperchad/transformer/src/lib.rs:2826` - SKIP_DEFAULT_DEBUG_ATTRS ✅ MIGRATED (supports "1" and "true")
- [x] `packages/hyperchad/transformer/src/lib.rs:3424` - DEBUG_ATTRS ✅ MIGRATED (supports "1" and "true")
- [x] `packages/hyperchad/transformer/src/lib.rs:3430` - DEBUG_RAW_ATTRS ✅ MIGRATED (supports "1" and "true")
- [x] `packages/hyperchad/js_bundler/src/node.rs:36` - PNPM_HOME ✅ MIGRATED (build tool detection)

**📦 Technical Debt Cleanup:** ✅ COMPLETED

- [x] `packages/env_utils/src/lib.rs:142-452` - Runtime functions removed, compile-time macros preserved ✅ COMPLETED

#### 1.4 Fix remaining direct time/instant usage ✅ COMPLETED

**Files modified:**

- [x] `packages/files/src/lib.rs:161,192` - Performance timing
- [x] `packages/audio_output/src/cpal.rs:596` - Audio timing
- [x] `packages/async/examples/simulated/src/main.rs:19` - SystemTime::now()
- [x] `packages/async/src/simulator/sync/mpmc/flume.rs:135` - Instant::now()
- [x] `packages/async/src/simulator/futures.rs:108-109` - SystemTime and Instant
- [x] `packages/async/src/simulator/mod.rs:260` - Instant::now()

All direct `std::time` usage has been migrated to use `switchy_time` functions.

#### 1.5 Add chrono DateTime support to `switchy_time` ✅ COMPLETED

- [x] Extend `packages/time/src/lib.rs` with DateTime abstractions
- [x] Add `datetime_local_now()` and `datetime_utc_now()` returning chrono DateTime types
- [x] Implement timezone-aware time functions for both standard and simulator modes
- [x] Migrate `packages/yt/src/lib.rs:1814` - chrono::Local::now()
- [x] Migrate `packages/database/src/postgres/postgres.rs:1601` - Utc::now()

**Added chrono support behind optional "chrono" feature:**

- Standard mode: Direct passthrough to chrono::Local::now() and chrono::Utc::now()
- Simulator mode: Deterministic DateTime generation based on simulated SystemTime

These tasks have no interdependencies and can execute simultaneously.

### Phase 2: File System & Ordering

**Goal: Fix ordering issues that affect all packages**

**Parallel execution possible:**

#### 2.1 Sort all `fs::read_dir` operations ✅ COMPLETED

**Files modified (9 occurrences):**

- [x] `packages/scan/src/output.rs:50` - Cover file scanning
- [x] `packages/scan/src/local.rs:566` - Directory scanning
- [x] `packages/files/src/lib.rs:450` - Cover directory reading
- [x] `packages/hyperchad/app/src/renderer.rs:452` - Resource copying
- [x] `packages/hyperchad/renderer/vanilla_js/build.rs:118` - Build script
- [x] `packages/clippier/tests/command_tests.rs:173,192` - Test utilities (already sorted)
- [x] `packages/clippier/test_utilities/src/lib.rs:192` - Test helpers
- [x] `packages/clippier/src/test_utils.rs:223` - Test utilities

**Implementation:** Added sorting after collecting entries for deterministic directory iteration

#### 2.2 Create deterministic file iteration helpers

- [ ] Add to `switchy_fs` package: `read_dir_sorted()`, `walk_dir_sorted()`

### Phase 3: Web Server Preparation

**Goal: Minimize rework during web server migration**

**Execution order:**

#### 3.1 Design trait abstractions for web concepts

**Create in `packages/web_server/src/traits/`:**

- [ ] `request.rs` - Request trait abstracting HttpRequest
- [ ] `response.rs` - Response trait abstracting HttpResponse
- [ ] `extractors.rs` - Data extraction traits (Path, Query, Json, etc.)
- [ ] `middleware.rs` - Middleware trait abstraction
- [ ] `service.rs` - Service factory traits

#### 3.2 Implement traits (parallel after 3.1)

**actix-web implementations:**

- [ ] Create `packages/web_server/src/actix/` module
- [ ] Implement all traits for actix types

**moosicbox_web_server implementations:**

- [ ] Enhance existing `packages/web_server/src/`
- [ ] Add missing features from Section 1 checklist

**Missing features to add (from Section 1):**

- [ ] WebSocket support (critical for 5 packages)
- [ ] Server-sent events
- [ ] Multipart form handling
- [ ] Custom error responses
- [ ] Request guards/extractors
- [ ] Middleware system
- [ ] Static file serving
- [ ] CORS configuration

#### 3.3 Build compatibility layer

- [ ] Migration helpers in `packages/web_server/src/migration/`
- [ ] Automated code transformation tools
- [ ] Dual-mode operation support

#### 3.4 Apply to leaf packages (proof of concept)

**Start with simplest packages:**

- [ ] `packages/config/src/api/` (simple REST)
- [ ] `packages/scan/src/api.rs` (basic endpoints)
- [ ] `packages/menu/src/api.rs` (no WebSockets)

The trait design must complete before implementations begin. See Section 1 "Web Server Framework" for detailed feature requirements.

### Phase 4: Web Server Migration

**Goal: Systematic migration with minimal disruption**

This phase executes the migration strategy detailed in Section 1.

**Total scope: 75 files across 35+ packages** (64 with actix types + 11 WebSocket-specific)

**Parallel migration groups** (no interdependencies):

#### 4.1 Auth/Config/Profiles group

**Files to migrate:**

- [ ] `packages/auth/src/lib.rs` - FromRequest implementations
- [ ] `packages/auth/src/api.rs` - Auth endpoints
- [ ] `packages/config/src/api/mod.rs` - Config service bindings
- [ ] `packages/profiles/src/lib.rs` - Profile management
- [ ] `packages/database/src/profiles.rs` - Database profile extractors
- [ ] `packages/database/src/config.rs` - Database config extractors
- [ ] `packages/library/music_api/src/profiles.rs` - Music API profiles
- [ ] `packages/music_api/src/profiles.rs` - Music profiles

#### 4.2 Media API group

**Files to migrate:**

- [ ] `packages/music_api/api/src/api.rs` - Core music API
- [ ] `packages/library/src/api.rs` - Library endpoints
- [ ] `packages/scan/src/api.rs` - Scan endpoints (#[actix_web::get] macros)
- [ ] `packages/search/src/api.rs` - Search endpoints
- [ ] `packages/qobuz/src/api.rs` - Qobuz API endpoints
- [ ] `packages/qobuz/src/lib.rs` - Qobuz types (HttpResponse)
- [ ] `packages/tidal/src/api.rs` - Tidal API endpoints
- [ ] `packages/tidal/src/lib.rs` - Tidal types (HttpResponse)
- [ ] `packages/yt/src/api.rs` - YouTube API endpoints
- [ ] `packages/yt/src/lib.rs` - YouTube types (HttpResponse)

#### 4.3 UI/Admin group

**Files to migrate:**

- [ ] `packages/admin_htmx/src/api/mod.rs` - Main HTMX endpoints
- [ ] `packages/admin_htmx/src/api/info.rs` - Info endpoints
- [ ] `packages/admin_htmx/src/api/profiles.rs` - Profile UI
- [ ] `packages/admin_htmx/src/api/qobuz.rs` - Qobuz UI
- [ ] `packages/admin_htmx/src/api/scan.rs` - Scan UI
- [ ] `packages/admin_htmx/src/api/tidal.rs` - Tidal UI
- [ ] `packages/menu/src/api.rs` - Menu API

#### 4.4 Network/Files group

**Files to migrate:**

- [ ] `packages/upnp/src/api.rs` - UPnP discovery endpoints
- [ ] `packages/downloader/src/api/mod.rs` - Download management
- [ ] `packages/files/src/api.rs` - File serving (uses actix_files)
- [ ] `packages/tunnel/src/lib.rs` - Tunnel types (HttpRequest/Response)

#### 4.5 Audio/Player group

**Files to migrate:**

- [ ] `packages/audio_zone/src/api/mod.rs` - Zone management
- [ ] `packages/audio_output/src/api/mod.rs` - Output control
- [ ] `packages/player/src/api.rs` - Player API (WebSocket critical)
- [ ] `packages/session/src/api/mod.rs` - Session API (WebSocket critical)

#### 4.6 Middleware group

**Files to migrate:**

- [ ] `packages/middleware/src/api_logger.rs` - API logging middleware
- [ ] `packages/middleware/src/service_info.rs` - Service info middleware
- [ ] `packages/middleware/src/tunnel_info.rs` - Tunnel info middleware
- [ ] `packages/telemetry/src/lib.rs` - Telemetry integration
- [ ] `packages/telemetry/src/simulator.rs` - Simulator telemetry

**Sequential requirements:**

#### 4.7 Hyperchad/Renderer group (complex WebSocket/SSE)

**Files to migrate:**

- [ ] `packages/hyperchad/renderer/html/actix/src/lib.rs` - Core actix renderer
- [ ] `packages/hyperchad/renderer/html/actix/src/actions.rs` - Action handlers
- [ ] `packages/hyperchad/renderer/html/actix/src/sse.rs` - Server-sent events
- [ ] `packages/hyperchad/renderer/html/src/actix.rs` - Actix integration
- [ ] `packages/hyperchad/renderer/html/src/web_server.rs` - Web server abstraction
- [ ] `packages/hyperchad/renderer/html/web_server/src/lib.rs` - Web server impl
- [ ] `packages/hyperchad/test_utils/src/http.rs` - HTTP test utilities
- [ ] `packages/hyperchad/test_utils/src/lib.rs` - Test utilities

#### 4.8 Core server package (depends on all above)

**Files to migrate:**

- [ ] `packages/server/src/lib.rs` - Main server setup (HttpServer, 50+ service bindings)
- [ ] `packages/server/src/api/mod.rs` - Core API module
- [ ] `packages/server/src/api/openapi.rs` - OpenAPI endpoints
- [ ] `packages/server/src/auth.rs` - Server auth
- [ ] `packages/server/src/ws/server.rs` - WebSocket server (critical)
- [ ] `packages/server/src/ws/handler.rs` - WebSocket handler (actix_ws)
- [ ] `packages/server/src/events/audio_zone_event.rs` - Audio zone events (WebSocket)
- [ ] `packages/server/src/events/session_event.rs` - Session events (WebSocket)
- [ ] `packages/server/src/players/local.rs` - Local player (WebSocket)
- [ ] `packages/server/src/players/upnp.rs` - UPnP player (WebSocket)
- [ ] `packages/server/simulator/src/host/moosicbox_server.rs` - Simulator server
- [ ] `packages/server/simulator/src/http.rs` - Simulator HTTP

#### 4.9 Tunnel server (after core server)

**Files to migrate:**

- [ ] `packages/tunnel_server/src/main.rs` - Main tunnel server (HttpServer)
- [ ] `packages/tunnel_server/src/api.rs` - Tunnel API endpoints
- [ ] `packages/tunnel_server/src/auth.rs` - Tunnel auth middleware
- [ ] `packages/tunnel_server/src/db.rs` - Database error handling
- [ ] `packages/tunnel_server/src/ws/api.rs` - Tunnel WebSocket API (actix_ws)
- [ ] `packages/tunnel_server/src/ws/handler.rs` - Tunnel WebSocket handler
- [ ] `packages/tunnel_sender/src/sender.rs` - WebSocket client side

#### 4.10 WebSocket core (critical for real-time)

**Files to migrate:**

- [ ] `packages/ws/src/ws.rs` - Core WebSocket utilities (WebsocketContext)
- [ ] All WebSocket files from 4.8 and 4.9 above (11 total files)

#### 4.11 Platform integrations

**Files to migrate:**

- [ ] `packages/app/tauri/src-tauri/src/lib.rs` - Tauri HTTP types
- [ ] `packages/web_server/src/actix.rs` - Actix compatibility layer
- [ ] `packages/web_server/src/lib.rs` - Web server core
- [ ] `packages/web_server/src/openapi.rs` - OpenAPI support

#### 4.12 Examples and tests

**Files to migrate:**

- [ ] `packages/simvar/examples/api_testing/src/main.rs` - API testing example
- [ ] `packages/simvar/examples/basic_web_server/src/main.rs` - Basic server example
- [ ] `packages/web_server/examples/nested_get/src/main.rs` - Nested routes example
- [ ] `packages/web_server/examples/openapi/src/main.rs` - OpenAPI example
- [ ] `packages/web_server/examples/simple_get/src/main.rs` - Simple GET example

#### 4.13 Final cleanup

- [ ] Remove actix-web, actix-ws, actix-files, actix-cors from all Cargo.toml files
- [ ] Update all imports and use statements
- [ ] Verify no remaining actix dependencies

**Migration complexity summary:**

- **High complexity (7+ files):** server, tunnel_server, hyperchad
- **Medium complexity (3-6 files):** admin_htmx, session, player, middleware
- **Low complexity (1-2 files):** Most API packages

**Critical WebSocket files (11 total):** Must maintain real-time functionality during migration

### Phase 5: Final Determinism

**Goal: Address remaining issues**

**Parallel execution possible:**

#### 5.1 Fix remaining async race conditions

**Focus areas:**

- [ ] WebSocket message ordering in `packages/ws/`, `packages/server/src/ws/`
- [ ] Player state updates in `packages/player/src/lib.rs`
- [ ] Session management in `packages/session/`
- [ ] Use `select_biased!` for deterministic future selection

**Specific files with select!/join!/spawn patterns:**

- [ ] `packages/server/src/ws/server.rs` - WebSocket server with select!
- [ ] `packages/player/src/lib.rs` - Player with tokio::spawn
- [ ] `packages/app/tauri/ws/src/lib.rs` - Tauri WebSocket
- [ ] `packages/app/state/src/ws.rs` - App WebSocket state
- [ ] `packages/upnp/src/player.rs` - UPnP player async operations

#### 5.2 Address floating-point determinism

**Specific files with float operations:**

- [ ] `packages/player/src/signal_chain.rs` - Signal processing chains
- [ ] `packages/player/src/volume_mixer.rs` - Volume mixing calculations
- [ ] `packages/player/src/symphonia.rs` - Audio decoding with floats
- [ ] `packages/player/src/symphonia_unsync.rs` - Unsync audio decoding
- [ ] `packages/player/src/local.rs` - Local player volume
- [ ] `packages/audio_output/src/lib.rs` - Output gain processing
- [ ] `packages/audio_output/src/cpal.rs` - CPAL audio output
- [ ] `packages/audio_zone/src/` - Zone volume management
- [ ] `packages/resampler/` - Audio resampling algorithms

Consider using fixed-point arithmetic or controlled rounding for determinism

#### 5.3 Update comprehensive documentation

- [ ] Update README.md with determinism guarantees
- [ ] Document all switchy packages and their usage
- [ ] Add examples for deterministic testing

#### 5.4 Final testing sweep

- [ ] Run full test suite with `SIMULATOR_*` variables
- [ ] Verify identical outputs across multiple runs
- [ ] Performance regression testing

### Phase 6: Process Abstraction

**Goal: Complete determinism by abstracting process execution**

**Note**: This phase is deferred to the end as it has the lowest impact on core determinism. Most process execution is in development tools, build scripts, or error handling paths.

#### 6.1 Create `switchy_process` package

- [ ] Create new package structure
- [ ] Implement command execution abstraction
- [ ] Add deterministic output for testing
- [ ] Implement process exit handling

#### 6.2 Migrate process execution calls

**Files needing migration (29 occurrences):**

- [ ] `packages/bloaty/src/main.rs:113` - Process exit
- [ ] `packages/server/src/lib.rs:769` - puffin_viewer launch
- [ ] `packages/hyperchad/renderer/egui/src/v1.rs:3780` - puffin_viewer
- [ ] `packages/hyperchad/js_bundler/src/node.rs` - Node.js command execution
- [ ] `packages/assert/src/lib.rs:25,44,183,200,221,267,325,358` - Assertion exits
- [ ] Build scripts: `tunnel_server`, `server`, `marketing_site`, `app/native`, `hyperchad/renderer/vanilla_js`

**Migration Priority**: Low - Most usage is in:

- Development tooling (bloaty, puffin_viewer)
- Build-time operations (git version info)
- Error handling (deterministic exits)
- Optional features (profiling)

### Phase 7: Deadlock Prevention (Optional)

**Goal: Prevent deadlocks in concurrent code**

**Note**: This phase is optional and focused on preventing deadlocks rather than improving determinism. Can be done after core DST work is complete.

#### 7.1 Document global lock hierarchy

- [ ] Create `LOCK_HIERARCHY.md` documenting all Arc<RwLock> usage
- [ ] Focus on: WebSocket connections, player state, cache maps
- [ ] Identify lock acquisition patterns and potential conflicts

#### 7.2 Add deadlock detection in debug builds

- [ ] Add deadlock detection to all RwLock acquisitions in debug mode
- [ ] Priority packages: `ws`, `server`, `tunnel_server`, `player`
- [ ] Add timeout-based deadlock detection for development

## Task Dependencies and Parallelization

### Independent Task Groups

These can execute in any order or simultaneously:

1. **Data Structure Determinism**

    - Collection replacements (HashMap → BTreeMap)
    - Sorting operations (fs::read_dir)
    - Lock ordering documentation

2. **Package Creation**

    - switchy_uuid ✅ COMPLETED
    - switchy_env ✅ COMPLETED

3. **Time Operations**
    - Instant replacements
    - SystemTime replacements
    - Chrono extensions

### Dependent Task Chains

These must execute in sequence:

1. **Web Abstraction Chain**

    - Design abstractions → Implement traits → Apply to packages → Migrate to new server

2. **UUID Chain**

    - Create switchy_uuid → Migrate auth tokens → Update session management

3. **Environment Chain**
    - Create switchy_env → Migrate critical vars → Update configuration loading

### Batch Processing Opportunities

- **Pattern replacements**: All HashMap→BTreeMap changes can happen at once
- **Import updates**: All package imports can update simultaneously
- **Sorting additions**: All read_dir operations can be fixed together
- **Mechanical changes**: Most find/replace operations can be parallelized

## Testing Strategy

### Unit Tests

- Mock all external dependencies using switchy packages
- Use seeded random for reproducible tests
- Verify deterministic ordering of operations

### Integration Tests

- Use simulator mode for full determinism
- Record and replay network interactions
- Validate consistent state across runs

### Simulation Tests

- Run with `SIMULATOR_*` environment variables
- Verify identical results across multiple runs
- Test with different seeds for coverage

## Conclusion

The MoosicBox codebase has made significant progress toward determinism with the switchy pattern. Key achievements include:

- ✅ **Environment variables**: 100% complete - 38+ variables migrated across 17+ packages + runtime functions removed
- ✅ **Time operations**: Most migrated (including new `instant_now()` support)
- ✅ **Random operations**: Complete using switchy_random
- ✅ **UUID generation**: Complete using switchy_uuid
- ✅ **Collections**: 100% complete - All HashMap/HashSet replaced with BTree variants
- ✅ **Legacy cleanup**: Runtime environment functions removed from moosicbox_env_utils

**CRITICAL DISCOVERY:** The single largest source of non-determinism is the direct use of actix-web throughout 50+ packages. However, by reordering tasks and maximizing parallelization, we can achieve significant determinism improvements while preparing for the web server migration.

## Execution Strategy Benefits

### Immediate Value

- Collections become deterministic immediately
- UUID determinism fixes security concerns early
- Testing becomes easier with each phase

### Minimal Rework

- Abstraction layer means touching files once
- Mechanical changes done early won't need revisiting
- Dependencies clearly mapped to avoid conflicts

### Continuous Progress

- Each phase delivers working improvements
- No "big bang" migration risk
- Can pause between phases if needed

## Optimized Approach Benefits

This execution strategy maximizes efficiency through:

- **Aggressive parallelization** of mechanical changes
- **Quick wins** that provide immediate value
- **Strategic ordering** to minimize rework
- **Clear dependency mapping** to enable parallel execution

The critical path remains the web server migration, but early determinism improvements will make that migration easier and more testable. Each phase builds on the previous one, creating a solid foundation for comprehensive determinism.

**Phase 6 (Process Abstraction)** is intentionally deferred to the end as it has the lowest impact on core application determinism. The 29 instances of process execution are primarily in development tools, build scripts, and error handling - areas that don't significantly affect the deterministic behavior of the main application logic.
