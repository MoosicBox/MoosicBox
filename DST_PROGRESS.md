# MoosicBox Determinism Audit

## Executive Summary

This document provides a comprehensive audit of non-deterministic patterns in the MoosicBox codebase. Each section identifies specific instances where determinism is not guaranteed and provides recommendations for remediation using the switchy pattern or other deterministic alternatives.

**CRITICAL FINDING:** The codebase has 50+ packages directly using actix-web instead of the deterministic moosicbox_web_server abstraction. This represents the single largest source of non-determinism and will require significant refactoring effort.

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

The current `moosicbox_web_server` needs significant enhancements to achieve feature parity with actix-web:

#### Missing Core Features

- [ ] **Extractors**: Json, Query, Path, Data, Header, Form
- [ ] **Middleware**: Logger, CORS, Compression, Authentication
- [ ] **WebSocket Support**: Full WS/WSS implementation
- [ ] **Streaming**: Server-sent events, chunked responses
- [ ] **Static Files**: File serving with range requests
- [ ] **Error Handling**: Custom error responses and handlers
- [ ] **State Management**: App data and request-scoped state
- [ ] **Guards**: Request guards and filters
- [ ] **Testing Utilities**: Test client and helpers

#### Ergonomic Improvements Needed

- [ ] **Macro-based routing**: Similar to actix's `#[get("/path")]`
- [ ] **Type-safe extractors**: Automatic deserialization
- [ ] **Builder pattern**: Fluent API for server configuration
- [ ] **Async trait handlers**: Support for async fn handlers
- [ ] **Automatic OpenAPI generation**: From route definitions

### Migration Strategy

1. **Phase 1**: Enhance moosicbox_web_server with missing features
2. **Phase 2**: Create migration guide and compatibility layer
3. **Phase 3**: Migrate one package at a time, starting with leaf packages
4. **Phase 4**: Migrate core server packages
5. **Phase 5**: Remove actix-web dependency

### Estimated Effort

- **Enhancement of moosicbox_web_server**: 4-6 weeks
- **Migration of all packages**: 6-8 weeks
- **Testing and validation**: 2-3 weeks
- **Total**: 12-17 weeks (3-4 months)

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

**Status:** 🟢 Minor | ✅ Mostly Fixed

### Remaining Issues

| File                           | Line | Usage             | Status   |
| ------------------------------ | ---- | ----------------- | -------- |
| `packages/clippier/src/lib.rs` | 193  | Feature shuffling | ✅ Fixed |
| `packages/openport/src/lib.rs` | 85   | Port selection    | ✅ Fixed |

### Recommendation

Continue using `switchy_random` for all random operations

## 6. Time Operations

**Status:** 🟢 Minor | ✅ Mostly Fixed

### Fixed ✅

- WebSocket heartbeats now use `switchy_time::instant_now()`
- Performance measurements use `switchy_time::instant_now()`
- Timestamps use `switchy_time::now()`

### Remaining

| File                                | Line     | Usage              | Priority     |
| ----------------------------------- | -------- | ------------------ | ------------ |
| `packages/files/src/lib.rs`         | 161, 192 | Performance timing | 🟢 Minor     |
| `packages/audio_output/src/cpal.rs` | 596      | Audio timing       | 🟡 Important |

## 7. Environment Variables

**Status:** 🟡 Important | ❌ Needs design

### Critical Variables (75 occurrences)

| Category  | Variables                                | Usage              | Priority     |
| --------- | ---------------------------------------- | ------------------ | ------------ |
| Database  | `DATABASE_URL`, `DB_*`                   | Connection strings | 🔴 Critical  |
| Security  | `TUNNEL_ACCESS_TOKEN`, `*_CLIENT_SECRET` | Authentication     | 🔴 Critical  |
| Simulator | `SIMULATOR_*`                            | Test configuration | 🟡 Important |
| Debug     | `DEBUG_*`, `TOKIO_CONSOLE`               | Debug flags        | 🟢 Minor     |

### Recommendation

Create `switchy_env` package for:

- Deterministic environment variable mocking
- Configuration injection for testing
- Validation and type-safe access

## 8. File System Operations

**Status:** 🟡 Important | ❌ Needs design

### Major Areas (100+ occurrences)

| Package                | Operation      | Usage           | Priority     |
| ---------------------- | -------------- | --------------- | ------------ |
| `packages/scan/`       | `fs::read_dir` | Music scanning  | 🔴 Critical  |
| `packages/files/`      | Directory ops  | File management | 🟡 Important |
| `packages/downloader/` | File writing   | Downloads       | 🟡 Important |
| `packages/clippier/`   | File I/O       | Build tools     | 🟢 Minor     |

### Recommendation

- Always sort directory listings before processing
- Use `switchy_fs` for mockable file operations
- Create deterministic file system abstraction for tests

## 9. Process/Command Execution

**Status:** 🟡 Important | ❌ Needs design

### Occurrences (29 instances)

| File                             | Command         | Usage          | Priority     |
| -------------------------------- | --------------- | -------------- | ------------ |
| `packages/bloaty/src/main.rs`    | `cargo`         | Build analysis | 🟢 Minor     |
| `packages/server/src/lib.rs:769` | `puffin_viewer` | Profiling      | 🟢 Minor     |
| `build.rs` files                 | `git`           | Version info   | 🟢 Minor     |
| `packages/assert/src/lib.rs`     | `process::exit` | Error handling | 🟡 Important |

### Recommendation

Create `switchy_process` for:

- Command execution mocking
- Deterministic exit handling
- Process output capture

## 10. Network Operations

**Status:** 🔴 Critical | ❌ Needs comprehensive mocking

### Major Areas (100+ occurrences)

| Package                   | Operation       | Usage              | Priority     |
| ------------------------- | --------------- | ------------------ | ------------ |
| `packages/tcp/`           | TCP connections | Core networking    | 🔴 Critical  |
| `packages/http/`          | HTTP requests   | API calls          | 🔴 Critical  |
| `packages/tunnel_sender/` | Tunnel requests | Remote connections | 🔴 Critical  |
| `packages/upnp/`          | UPnP discovery  | Device discovery   | 🟡 Important |
| `packages/openport/`      | Port binding    | Port allocation    | 🟡 Important |

### Recommendation

Leverage existing `switchy_tcp` and `switchy_http` more extensively

## 11. Async Race Conditions

**Status:** 🔴 Critical | ❌ Needs careful analysis

### Problem Areas

| Pattern                     | Count | Risk                         | Priority     |
| --------------------------- | ----- | ---------------------------- | ------------ |
| `.await.unwrap()`           | 100+  | Panic on error               | 🟡 Important |
| `join_all` without ordering | 15+   | Non-deterministic completion | 🔴 Critical  |
| `select()` in handlers      | 10+   | Race conditions              | 🔴 Critical  |
| Concurrent DB ops           | 20+   | Data races                   | 🔴 Critical  |

### Recommendation

- Use deterministic task ordering where possible
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

## Priority Roadmap

### Phase 0: Web Server Migration (🔴🔴🔴 HIGHEST PRIORITY)

1. [ ] Enhance moosicbox_web_server with missing core features
    - [ ] Implement extractors (Json, Query, Path, Data, Header)
    - [ ] Add middleware support (Logger, CORS, Compression)
    - [ ] Implement WebSocket support
    - [ ] Add streaming and SSE support
    - [ ] Implement static file serving with range requests
2. [ ] Create migration tooling and compatibility layer
3. [ ] Migrate leaf packages (lowest dependencies first)
4. [ ] Migrate core server packages
5. [ ] Remove actix-web from workspace

### Phase 1: Critical Security & Core (🔴)

1. [ ] Create `switchy_uuid` for deterministic UUID generation
2. [ ] Replace remaining HashMap/HashSet in core packages
3. [ ] Fix async race conditions in critical paths
4. [ ] Establish lock ordering hierarchy

### Phase 2: Testing & Reliability (🟡)

1. [ ] Extend `switchy_time` for chrono support
2. [ ] Create `switchy_env` for environment variables
3. [ ] Implement deterministic file system operations
4. [ ] Add network operation mocking

### Phase 3: Complete Determinism (🟢)

1. [ ] Create `switchy_process` for command execution
2. [ ] Address floating-point determinism where needed
3. [ ] Complete migration of all time operations
4. [ ] Document acceptable non-determinism

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

**CRITICAL DISCOVERY:** The single largest source of non-determinism is the direct use of actix-web throughout 50+ packages. This must be addressed before other determinism efforts can be fully effective.

Major remaining work (in priority order):

- 🔴🔴🔴 **Web Server Migration** - Replace actix-web with moosicbox_web_server (12-17 weeks)
- 🔴 UUID generation needs abstraction
- 🔴 Network operations need comprehensive mocking
- 🔴 Async race conditions need resolution
- 🟡 File system operations need deterministic ordering
- 🟡 Environment variables need abstraction

**Revised Total Estimated Effort:**

- Phase 0 (Web Server): 12-17 weeks
- Phase 1 (Critical): 2-3 weeks
- Phase 2 (Testing): 3-4 weeks
- Phase 3 (Complete): 2-3 weeks
- **Total: 19-27 weeks (5-7 months)**

The web server migration represents approximately 60-70% of the total determinism effort and blocks many other improvements. This should be the immediate focus.
