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

**Status:** 🔴 Critical | ❌ Blocked (no switchy_uuid implementation)

### Occurrences

| File                                               | Line     | Usage                     | Priority    |
| -------------------------------------------------- | -------- | ------------------------- | ----------- |
| `packages/tunnel_server/src/api.rs`                | 110, 129 | Token generation          | 🔴 Critical |
| `packages/auth/src/lib.rs`                         | 75, 88   | Magic token & session IDs | 🔴 Critical |
| `packages/simvar/examples/api_testing/src/main.rs` | 276, 398 | Test data                 | 🟢 Minor    |

### Recommendation

Create `switchy_uuid` package with:

- Deterministic UUID generation for testing
- Cryptographically secure UUIDs for production
- Seeded UUID generation for simulations

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

**Status:** 🟡 Important | ⏳ In Progress

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

**Status:** 🟡 Important | ❌ No abstraction exists

### Problem

Direct use of `std::env::var` throughout codebase without abstraction. Need to create `switchy_env` package.

### Direct Usage (75 occurrences)

| Category  | Variables                                | Usage              | Priority     |
| --------- | ---------------------------------------- | ------------------ | ------------ |
| Database  | `DATABASE_URL`, `DB_*`                   | Connection strings | 🔴 Critical  |
| Security  | `TUNNEL_ACCESS_TOKEN`, `*_CLIENT_SECRET` | Authentication     | 🔴 Critical  |
| Simulator | `SIMULATOR_*`                            | Test configuration | 🟡 Important |
| Debug     | `DEBUG_*`, `TOKIO_CONSOLE`               | Debug flags        | 🟢 Minor     |

### Recommendation

Create new `switchy_env` package with:

- Environment variable abstraction
- Deterministic values for testing
- Configuration injection
- Type-safe access patterns

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

**Parallel execution possible:**

#### 1.1 Replace ALL remaining HashMap/HashSet with BTreeMap/BTreeSet

**Files to modify (89 occurrences across 30 files):**

- [x] `packages/scan/src/output.rs:583,620,664` - HashSet<u64> for IDs
- [x] `packages/server/src/ws/server.rs:132,137,141` - Connection maps
- [x] `packages/server/src/auth.rs:108` - Query parameter collection
- [x] `packages/server/src/players/upnp.rs:23` - Player state map
- [x] `packages/ws/src/ws.rs:86` - CONNECTION_DATA static
- [x] `packages/player/src/api.rs:125` - PlaybackHandler map
- [x] `packages/player/src/lib.rs:228,364,365` - Query/headers maps
- [x] `packages/hyperchad/state/src/store.rs:14` - Cache storage
- [ ] `packages/hyperchad/renderer/egui/src/v1.rs:229-777` - UI state maps (15+ occurrences)
- [ ] `packages/hyperchad/renderer/egui/src/v2.rs:178-180,507` - UI element maps
- [ ] `packages/hyperchad/renderer/fltk/src/lib.rs:284` - Image cache
- [ ] `packages/hyperchad/renderer/src/lib.rs:298,308` - Headers parameters
- [ ] `packages/hyperchad/renderer/vanilla_js/src/lib.rs:798,815` - Headers
- [ ] `packages/hyperchad/renderer/html/src/lib.rs:49,257,268` - Responsive triggers
- [ ] `packages/hyperchad/renderer/html/src/actix.rs:267` - Static headers
- [ ] `packages/hyperchad/renderer/html/src/html.rs:1046` - Headers
- [ ] `packages/hyperchad/renderer/html/src/lambda.rs:253` - Lambda headers
- [ ] `packages/hyperchad/renderer/html/src/web_server.rs:233` - Web server headers
- [ ] `packages/hyperchad/renderer/html/http/src/lib.rs:95` - HTTP headers
- [ ] `packages/hyperchad/actions/src/dsl.rs:448` - DSL variables
- [x] `packages/tunnel_server/src/ws/server.rs:332,343-352,530` - WebSocket state
- [x] `packages/tunnel_sender/src/sender.rs:187,275,619-1013` - Request tracking
- [x] `packages/tunnel/src/lib.rs:46` - Tunnel headers
- [x] `packages/files/src/files/track_pool.rs:85-86` - Semaphore/pool maps
- [x] `packages/upnp/src/listener.rs:68-69` - Status tracking
- [x] `packages/load_balancer/src/server.rs:27,43,66` - Cluster configuration
- [x] `packages/load_balancer/src/load_balancer.rs:35,39` - Router maps
- [x] `packages/app/tauri/src-tauri/src/lib.rs:1220,1270,1284` - Headers and state
- [ ] `packages/app/native/src/visualization.rs:227` - Visualization cache
- [x] `packages/app/state/src/lib.rs:225,231,1165,1182,1200` - Audio zone and player state

#### 1.2 Create `switchy_uuid` package

- [ ] Create new package structure
- [ ] Implement deterministic UUID generation for testing
- [ ] Implement cryptographically secure UUIDs for production
- [ ] Add seeded UUID generation for simulations

**Files needing migration (6 direct usages):**

- [ ] `packages/tunnel_server/src/api.rs:27,110,129` - Token generation
- [ ] `packages/auth/src/lib.rs:16,75,88` - Magic token generation
- [ ] `packages/simvar/examples/api_testing/src/main.rs:20,276,398` - Test IDs

#### 1.3 Create `switchy_env` package

- [ ] Create new package structure
- [ ] Implement environment variable abstraction
- [ ] Add deterministic values for testing
- [ ] Implement configuration injection
- [ ] Add type-safe access patterns

**Files needing migration (58 occurrences):**

- [ ] `packages/database_connection/src/creds.rs:38-78` - Database credentials
- [ ] `packages/env_utils/src/lib.rs:142-452` - All env utilities
- [ ] `packages/auth/src/lib.rs:120` - TUNNEL_ACCESS_TOKEN
- [ ] `packages/app/native/ui/src/api/tidal.rs:16,65-66` - Tidal credentials
- [ ] `packages/simvar/harness/src/lib.rs:52,115` - Simulator config
- [ ] `packages/simvar/harness/src/config.rs:55,377` - Simulator settings
- [ ] `packages/time/src/simulator.rs:26,63` - Time offsets
- [ ] `packages/random/src/simulator.rs:13,48` - Random seeds
- [ ] `packages/load_balancer/src/server.rs:44,81` - SSL configuration
- [ ] `packages/load_balancer/src/load_balancer.rs:12,19,26,30` - Port/SSL paths
- [ ] Build scripts and main.rs files (10+ occurrences)

#### 1.4 Create `switchy_process` package

- [ ] Create new package structure
- [ ] Implement command execution abstraction
- [ ] Add deterministic output for testing
- [ ] Implement process exit handling

**Files needing migration (17 occurrences):**

- [ ] `packages/bloaty/src/main.rs:113` - Process exit
- [ ] `packages/server/src/lib.rs:769` - puffin_viewer launch
- [ ] `packages/hyperchad/renderer/egui/src/v1.rs:3780` - puffin_viewer
- [ ] `packages/hyperchad/js_bundler/src/node.rs` - Node.js command execution
- [ ] `packages/assert/src/lib.rs:25,44,183,200,221,267,325,358` - Assertion exits
- [ ] Build scripts: `tunnel_server`, `server`, `marketing_site`, `app/native`, `hyperchad/renderer/vanilla_js`

#### 1.5 Fix remaining direct time/instant usage

**Files to modify:**

- [ ] `packages/files/src/lib.rs:161,192` - Performance timing
- [ ] `packages/audio_output/src/cpal.rs:596` - Audio timing
- [ ] `packages/async/examples/simulated/src/main.rs:19` - SystemTime::now()
- [ ] `packages/async/src/simulator/sync/mpmc/flume.rs:135` - Instant::now()
- [ ] `packages/async/src/simulator/futures.rs:108-109` - SystemTime and Instant
- [ ] `packages/async/src/simulator/mod.rs:260` - Instant::now()

Note: Files in `packages/time/src/` are part of the switchy_time implementation itself

#### 1.6 Add chrono DateTime support to `switchy_time`

- [ ] Extend `packages/time/src/lib.rs` with DateTime abstractions
- [ ] Add `datetime_now()` returning chrono DateTime types
- [ ] Implement timezone-aware time functions
- [ ] Add date formatting utilities

These tasks have no interdependencies and can execute simultaneously.

### Phase 2: File System & Ordering

**Goal: Fix ordering issues that affect all packages**

**Parallel execution possible:**

#### 2.1 Sort all `fs::read_dir` operations

**Files to modify (9 occurrences):**

- [ ] `packages/scan/src/output.rs:50` - Cover file scanning
- [ ] `packages/scan/src/local.rs:566` - Directory scanning
- [ ] `packages/files/src/lib.rs:450` - Cover directory reading
- [ ] `packages/hyperchad/app/src/renderer.rs:452` - Resource copying
- [ ] `packages/hyperchad/renderer/vanilla_js/build.rs:118` - Build script
- [ ] `packages/clippier/tests/command_tests.rs:173,192` - Test utilities
- [ ] `packages/clippier/test_utilities/src/lib.rs:192` - Test helpers
- [ ] `packages/clippier/src/test_utils.rs:223` - Test utilities

**Implementation:** Add `.sort()` after collecting entries, migrate to `switchy_fs`

#### 2.2 Document global lock hierarchy

- [ ] Create `LOCK_HIERARCHY.md` documenting all Arc<RwLock> usage
- [ ] Focus on: WebSocket connections, player state, cache maps

#### 2.3 Add deadlock detection in debug builds

- [ ] Add to all RwLock acquisitions in debug mode
- [ ] Priority packages: `ws`, `server`, `tunnel_server`, `player`

#### 2.4 Create deterministic file iteration helpers

- [ ] Add to `switchy_fs` package: `read_dir_sorted()`, `walk_dir_sorted()`

These are mechanical changes that don't conflict with each other.

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

## Task Dependencies and Parallelization

### Independent Task Groups

These can execute in any order or simultaneously:

1. **Data Structure Determinism**

    - Collection replacements (HashMap → BTreeMap)
    - Sorting operations (fs::read_dir)
    - Lock ordering documentation

2. **Package Creation**

    - switchy_uuid
    - switchy_env
    - switchy_process

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

- ✅ Most time operations migrated (including new `instant_now()` support)
- ✅ Random operations using switchy_random
- ✅ Some collections migrated to BTree variants

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
