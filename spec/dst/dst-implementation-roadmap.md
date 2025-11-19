# DST Implementation Roadmap

**Purpose**: Track the systematic completion of DST web server abstraction tasks in the correct order.
**Generated**: 2025-01-21
**Source**: Extracted from plan.md uncompleted verification tasks
**Last Updated**: 2025-01-21

## Overview

This document provides a linear, actionable roadmap to complete the DST web server abstraction.
Each task group must be completed and verified before moving to the next.

The main plan.md shows implementation tasks as "COMPLETED" but verification checklists are unchecked.
This roadmap focuses on systematically verifying and completing the remaining work.

## Current Status Summary

- **Step 5.1**: SimulatorWebServer Basics - 94.5% complete (86/91 tasks)
    - Implementation: ✅ Claimed complete
    - Verification: 🔄 25/75 verification tasks complete (Phase 1.1 ✅, Phase 1.2 ✅)
- **Step 5.2**: TestClient Abstraction - 70% complete (19/27 tasks)
- **Step 5.3-5.6**: Not started
- **Critical Discovery**: Many "completed" sections have unchecked verification checklists

---

## Phase 1: Complete Section 5.1 Verification (IN PROGRESS - 2/5 phases complete)

**Goal**: Verify all completed implementation actually works
**Estimated Time**: 2-4 hours (1.5 hours completed)
**Risk**: Low - just running tests and verification commands
**Location**: plan.md lines 3891-4201
**Status**: ✅ Phase 1.1 Complete, ✅ Phase 1.2 Complete, 🔄 Phase 1.3 Ready

### 1.1 HttpResponse Header Support Verification (15 tasks)

**Location**: Section 5.1.5.1 Verification Checklist (lines 3891-3916)
**Priority**: HIGH - Foundation for response generation

#### Header Support Functionality (5 tasks)

- [x] **Verify headers field added to HttpResponse struct with BTreeMap<String, String>**

    ```bash
    nix develop --command grep -n "headers.*BTreeMap<String, String>" packages/web_server/src/lib.rs
    ```

    - **Expected**: Find headers field in HttpResponse struct
    - **Success**: Line showing `headers: BTreeMap<String, String>`

- [x] **Verify location field migrated to use headers map**

    ```bash
    nix develop --command grep -A5 -B5 "with_location" packages/web_server/src/lib.rs
    ```

    - **Expected**: with_location method sets both location field AND Location header
    - **Success**: Code shows dual setting pattern

- [x] **Test with_header() method adds individual headers correctly**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server test_with_header
    ```

    - **Expected**: Test passes showing headers are added
    - **Success**: Test passes with 0 failures

- [x] **Test with_headers() method sets multiple headers at once**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server test_with_headers
    ```

    - **Expected**: Test passes showing bulk header setting works
    - **Success**: Test passes with 0 failures

- [x] **Test with_content_type() helper method sets Content-Type header**
    ```bash
    nix develop --command cargo test -p moosicbox_web_server test_with_content_type
    ```

    - **Expected**: Test passes showing Content-Type header is set
    - **Success**: Test passes with 0 failures

#### Builder Methods (4 tasks)

- [x] **Test HttpResponse::json() sets application/json content-type**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server test_json_response
    ```

    - **Expected**: JSON responses have correct content-type
    - **Success**: Test passes confirming application/json header

- [x] **Test HttpResponse::html() sets text/html; charset=utf-8**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server test_html_response
    ```

    - **Expected**: HTML responses have correct content-type
    - **Success**: Test passes confirming text/html header

- [x] **Test HttpResponse::text() sets text/plain; charset=utf-8**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server test_text_response
    ```

    - **Expected**: Text responses have correct content-type
    - **Success**: Test passes confirming text/plain header

- [x] **Verify headers preserved when chaining builder methods**
    ```bash
    nix develop --command cargo test -p moosicbox_web_server test_header_chaining
    ```

    - **Expected**: Multiple chained calls preserve all headers
    - **Success**: Test passes confirming header preservation

#### Build & Compilation (3 tasks)

- [x] **Run `cargo build -p moosicbox_web_server` - Builds successfully**

    ```bash
    nix develop --command cargo build -p moosicbox_web_server
    ```

    - **Expected**: Finished [profile] target(s) with no errors
    - **Success**: Clean compilation with zero errors

- [x] **Run `cargo test --no-run -p moosicbox_web_server` - Tests compile**

    ```bash
    nix develop --command cargo test --no-run -p moosicbox_web_server
    ```

    - **Expected**: All tests compile successfully
    - **Success**: Finished compilation with no errors

- [x] **Backwards compatibility maintained with existing code**
    ```bash
    nix develop --command cargo test -p moosicbox_web_server http_response
    ```

    - **Expected**: All existing HttpResponse tests pass
    - **Success**: All tests pass, no regressions

#### Code Quality (3 tasks)

- [x] **Run `cargo fmt` - Code properly formatted**

    ```bash
    nix develop --command cargo fmt --check
    ```

    - **Expected**: No output (all files properly formatted)
    - **Success**: Command exits cleanly

- [x] **Run `cargo clippy -p moosicbox_web_server -- -D warnings` - Zero warnings**

    ```bash
    nix develop --command cargo clippy -p moosicbox_web_server -- -D warnings
    ```

    - **Expected**: Finished with zero warnings
    - **Success**: Clean clippy output

- [x] **Run `cargo machete` - No unused dependencies**
    ```bash
    nix develop --command cargo machete
    ```

    - **Expected**: No unused dependencies found
    - **Success**: Clean output or no unused deps

**Section 1.1 Complete When**: All 15 checkboxes above are marked complete ✅ **COMPLETED**

---

### 1.2 Response Generation Verification (10 tasks)

**Location**: Section 5.1.5 Verification Checklist (lines 4028-4050)
**Priority**: HIGH - Critical for request/response cycle

#### Response Generation Functionality (5 tasks)

- [x] **Test HttpResponse::json() sets correct content-type header**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server --features simulator test_json_response_conversion
    ```

    - **Expected**: JSON conversion preserves content-type
    - **Success**: test_json_response_conversion_preserves_content_type passes

- [x] **Test HttpResponse::html() sets text/html content-type**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server --features simulator test_html_response_conversion
    ```

    - **Expected**: HTML responses have correct content-type
    - **Success**: test_html_response_conversion passes

- [x] **Test HttpResponse::text() sets text/plain content-type**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server --features simulator test_text_response_conversion
    ```

    - **Expected**: Text responses have correct content-type
    - **Success**: test_text_response_conversion passes

- [x] **Test status codes preserved in conversion (200, 404, 500, etc.)**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server --features simulator test_status_codes_are_preserved
    ```

    - **Expected**: All status codes correctly converted
    - **Success**: test_status_codes_are_preserved passes

- [x] **Test custom headers preserved without modification**
    ```bash
    nix develop --command cargo test -p moosicbox_web_server --features simulator test_custom_headers_are_preserved
    ```

    - **Expected**: Custom headers maintained in conversion
    - **Success**: test_custom_headers_are_preserved passes

#### Build & Compilation (2 tasks)

- [x] **Run `cargo build -p moosicbox_web_server --features simulator` - Builds successfully**

    ```bash
    nix develop --command cargo build -p moosicbox_web_server --features simulator
    ```

    - **Expected**: Clean compilation with simulator feature
    - **Success**: Finished with zero errors

- [x] **Run `cargo test --no-run -p moosicbox_web_server --features simulator` - Tests compile**
    ```bash
    nix develop --command cargo test --no-run -p moosicbox_web_server --features simulator
    ```

    - **Expected**: All tests compile with simulator feature
    - **Success**: Finished compilation with no errors

#### Code Quality (3 tasks)

- [x] **Run `cargo fmt` - Code properly formatted**

    ```bash
    nix develop --command cargo fmt --check
    ```

    - **Expected**: No formatting issues
    - **Success**: Command exits cleanly

- [x] **Run `cargo clippy -p moosicbox_web_server --features simulator -- -D warnings` - Zero warnings**

    ```bash
    nix develop --command cargo clippy -p moosicbox_web_server --features simulator -- -D warnings
    ```

    - **Expected**: Zero clippy warnings with simulator
    - **Success**: Clean clippy output

- [x] **Run `cargo machete` - No unused dependencies**
    ```bash
    nix develop --command cargo machete
    ```

    - **Expected**: No unused dependencies
    - **Success**: Clean output

**Section 1.2 Complete When**: All 10 checkboxes above are marked complete ✅ **COMPLETED**

---

### 1.3 State Management Verification (10 tasks)

**Location**: Section 5.1.6 Verification Checklist (lines 4080-4101)
**Priority**: MEDIUM - Required for real applications

#### State Management Functionality (5 tasks)

- [ ] **Test insert_state<T>() stores typed state in StateContainer**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server --features simulator test_insert_and_retrieve_string
    ```

    - **Expected**: String state insertion and retrieval works
    - **Success**: test_simulator_state_management_string_state passes

- [ ] **Test get_state<T>() retrieves state with correct type**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server --features simulator test_insert_and_retrieve_struct
    ```

    - **Expected**: Custom struct state works
    - **Success**: test_simulator_state_management_custom_struct_state passes

- [ ] **Test state shared across multiple requests (Arc<RwLock> pattern)**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server --features simulator test_state_shared_across_requests
    ```

    - **Expected**: State persists across request boundaries
    - **Success**: test_simulator_state_management_shared_across_requests passes

- [ ] **Test State<T> extractor works with simulator backend**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server --features simulator test_state_extractor_integration
    ```

    - **Expected**: State<T> extractor extracts from StateContainer
    - **Success**: test_simulator_state_management_handler_extraction passes

- [ ] **Verify thread-safe concurrent access to state**
    ```bash
    nix develop --command cargo test -p moosicbox_web_server --features simulator test_state_multiple_types
    ```

    - **Expected**: Multiple state types work concurrently
    - **Success**: test_simulator_state_management_multiple_types passes

#### Build & Compilation (2 tasks)

- [ ] **Run `cargo build -p moosicbox_web_server --features simulator` - Builds successfully**
- [ ] **Run `cargo test --no-run -p moosicbox_web_server --features simulator` - Tests compile**

#### Code Quality (3 tasks)

- [ ] **Run `cargo fmt` - Code properly formatted**
- [ ] **Run `cargo clippy -p moosicbox_web_server --features simulator -- -D warnings` - Zero warnings**
- [ ] **Run `cargo machete` - No unused dependencies**

**Section 1.3 Complete When**: All 10 checkboxes above are marked complete

---

### 1.4 Scope Processing Verification (10 tasks)

**Location**: Section 5.1.7 Verification Checklist (lines 4124-4148)
**Priority**: HIGH - Required for nested API structures

#### Scope Processing Functionality (5 tasks)

- [ ] **Test register_scope() processes scope prefix correctly**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server --features simulator test_register_scope_with_single_route
    ```

    - **Expected**: Single scope routes work with prefixes
    - **Success**: test_register_scope_with_single_route passes

- [ ] **Test routes within scope have prefix prepended**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server --features simulator test_register_scope_with_multiple_routes
    ```

    - **Expected**: Multiple routes get scope prefix
    - **Success**: test_register_scope_with_multiple_routes passes

- [ ] **Test nested scopes combine prefixes properly**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server --features simulator test_register_scope_with_nested_scopes
    ```

    - **Expected**: Nested scopes create correct full paths
    - **Success**: test_register_scope_with_nested_scopes passes

- [ ] **Test empty prefix scopes handled correctly**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server --features simulator test_register_scope_with_empty_prefix
    ```

    - **Expected**: Empty scopes don't break path generation
    - **Success**: test_register_scope_with_empty_prefix passes

- [ ] **Test deep nesting (3+ levels) works correctly**
    ```bash
    nix develop --command cargo test -p moosicbox_web_server --features simulator test_register_scope_with_deeply_nested_scopes
    ```

    - **Expected**: Deep nesting creates correct paths
    - **Success**: test_register_scope_with_deeply_nested_scopes passes

#### Build & Compilation (2 tasks)

- [ ] **Run `cargo build -p moosicbox_web_server --features simulator` - Builds successfully**
- [ ] **Run `cargo test --no-run -p moosicbox_web_server --features simulator` - Tests compile**

#### Code Quality (3 tasks)

- [ ] **Run `cargo fmt` - Code properly formatted**
- [ ] **Run `cargo clippy -p moosicbox_web_server --features simulator -- -D warnings` - Zero warnings**
- [ ] **Run `cargo machete` - No unused dependencies**

**Section 1.4 Complete When**: All 10 checkboxes above are marked complete

---

### 1.5 Integration Testing Verification (10 tasks)

**Location**: Section 5.1.8 Verification Checklist (lines 4176-4201)
**Priority**: HIGH - Proves end-to-end functionality

#### Integration Test Coverage (6 tasks)

- [ ] **Test multiple HTTP methods (GET, POST, PUT, DELETE)**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server --features simulator test_multiple_http_methods
    ```

    - **Expected**: All HTTP methods work through integration
    - **Success**: Integration tests show all methods working

- [ ] **Test complex path parameters work end-to-end**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server --features simulator test_complex_path_parameters
    ```

    - **Expected**: Complex paths like /users/{id}/posts/{post_id} work
    - **Success**: Path parameter extraction working

- [ ] **Test multiple extractors work together (Query, Json, Path)**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server --features simulator test_multiple_extractors
    ```

    - **Expected**: Combined extractors work in single handler
    - **Success**: Multi-extractor tests pass

- [ ] **Test state extraction works in real handlers**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server --features simulator test_state_in_handlers
    ```

    - **Expected**: State<T> extractor works in integration tests
    - **Success**: State extraction in real handlers works

- [ ] **Test 404 handling for unmatched routes works**

    ```bash
    nix develop --command cargo test -p moosicbox_web_server --features simulator test_404_handling
    ```

    - **Expected**: Unmatched routes return 404
    - **Success**: 404 responses generated correctly

- [ ] **Test deterministic execution order validated**
    ```bash
    nix develop --command cargo test -p moosicbox_web_server --features simulator test_deterministic_execution
    ```

    - **Expected**: Requests process in deterministic order
    - **Success**: Order validation tests pass

#### Build & Compilation (1 task)

- [ ] **Run `cargo build --example basic_simulation --features simulator` - Example compiles**
    ```bash
    nix develop --command cargo build --example basic_simulation --features simulator
    ```

    - **Expected**: Example compiles successfully
    - **Success**: Basic simulation example builds

#### Code Quality (3 tasks)

- [ ] **Run `cargo fmt` - Code properly formatted**
- [ ] **Run `cargo clippy -p moosicbox_web_server --features simulator -- -D warnings` - Zero warnings**
- [ ] **Run `cargo machete` - No unused dependencies**

**Section 1.5 Complete When**: All 10 checkboxes above are marked complete

---

## Phase 1 Success Criteria

**Phase 1 is COMPLETE when**:

- ✅ All 55 verification tasks above are checked off
- ✅ All tests pass with zero failures
- ✅ Zero clippy warnings across the package
- ✅ All build commands succeed
- ✅ No regressions found in existing functionality

**Estimated Time**: 2-4 hours of verification work
**Risk Level**: LOW - mostly running existing tests
**Blockers**: None - this is verification of existing implementation

---

## Phase 2: Complete Section 5.2 Remaining Tasks

**Goal**: Finish TestClient abstraction to enable unified testing
**Estimated Time**: 4-6 hours
**Risk**: Medium - Some architectural decisions needed
**Prerequisites**: Phase 1 must be 100% complete

### 2.1 Fix Section 5.2.4.3.1 RequestContext Infrastructure

**Status**: Partially implemented, needs completion
**Blocker for**: Route parameters and Path extractor
**Location**: plan.md lines 5700+

#### Tasks Remaining:

- [ ] Complete RequestContext structure implementation
- [ ] Refactor HttpRequest enum to include context
- [ ] Add request-scoped data accessors
- [ ] Update Path extractor to use RequestContext
- [ ] Test with parameterized routes like `/users/{id}`

### 2.2 Complete Section 5.2.4 Remaining Sub-tasks

**Priority Order**:

1. [ ] **5.2.4.3**: Route Parameters & Pattern Matching (HIGH - enables dynamic routes)
2. [ ] **5.2.4.4**: State management integration (MEDIUM - needed for real apps)
3. [ ] **5.2.4.5**: Middleware system integration (MEDIUM - auth, logging, etc.)
4. [ ] **5.2.4.7**: Builder addr/port configuration (LOW - convenience feature)
5. [ ] **5.2.4.8**: Final integration and cleanup (LOW - polish)

### 2.3 Complete Section 5.2 Remaining Verification Tasks

**Identified gaps**:

- TestClient factory layer needs completion
- ActixTestClient real server integration needs finishing
- Unified API needs final polish

---

## Phase 3: Architecture Completion (Section 5.3-5.4)

**Goal**: Complete web server abstraction with unified API
**Estimated Time**: 8-12 hours
**Risk**: High - Major architectural work
**Prerequisites**: Phase 2 must be 100% complete

### 3.1 Unified WebServer Trait

- [ ] Define trait interface for both backends
- [ ] Implement for SimulatorWebServer
- [ ] Implement for ActixWebServer
- [ ] Unified startup/shutdown methods

### 3.2 Complete SimulatorWebServer

- [ ] Middleware pipeline support
- [ ] Async executor integration
- [ ] Request/response recording for testing

### 3.3 Remove Feature Gates from Examples

- [ ] Update all examples to use unified API
- [ ] Create migration guide for existing code
- [ ] Document usage patterns

---

## Tracking Format

Each task should be tracked with:

- **Status**: ⏳ Pending | 🚧 In Progress | ✅ Complete | ❌ Blocked
- **Command**: The exact `nix develop --command` to run
- **Expected**: What the command should output for success
- **Actual**: What actually happened when run
- **Notes**: Any issues encountered or deviations

---

## Success Metrics

### Phase 1 Complete When:

- ✅ All 55 verification tasks checked off
- ✅ Zero clippy warnings across all features
- ✅ All tests passing with both `--features simulator` and `--features actix`
- ✅ No regressions in existing functionality

### Phase 2 Complete When:

- ✅ TestClient works without feature gates in user code
- ✅ Both ActixTestClient and SimulatorTestClient fully functional
- ✅ Examples demonstrate unified API usage
- ✅ Route parameters work with Path extractor

### Phase 3 Complete When:

- ✅ Examples work without ANY feature gates
- ✅ Can run servers with unified `WebServer::builder().run()` API
- ✅ Migration guide complete and tested
- ✅ All step 5 completion criteria from plan.md satisfied

---

## Daily Progress Template

### [Date] Progress

**Started**: [Time]
**Working On**: Phase [X] - Section [X.X]

**Completed Tasks**:

- [ ] Task 1 with specific command and result
- [ ] Task 2 with specific command and result

**Current Status**:

- **Total Progress**: [X/125] tasks complete ([X]%)
- **Phase 1**: [X/55] verification tasks
- **Phase 2**: [X/35] implementation tasks
- **Phase 3**: [X/35] architecture tasks

**Blocked On**:

- [Issue description with specifics]

**Next Steps**:

- [Specific next task to work on]

**Time Spent**: [Hours]
**Notes**: [Any discoveries, issues, or decisions]

---

## Quick Commands Reference

### Core Build/Test Commands

```bash
# Build web server package
nix develop --command cargo build -p moosicbox_web_server

# Build with simulator feature
nix develop --command cargo build -p moosicbox_web_server --features simulator

# Run all tests
nix develop --command cargo test -p moosicbox_web_server

# Run simulator tests only
nix develop --command cargo test -p moosicbox_web_server --features simulator

# Check code quality
nix develop --command cargo clippy -p moosicbox_web_server -- -D warnings
nix develop --command cargo fmt --check
nix develop --command cargo machete
```

### Progress Checking

```bash
# Count completed verification tasks
grep -c "\[x\].*✅" /hdd/GitHub/mb-worktree/dst/spec/dst/dst-implementation-roadmap.md

# Count total verification tasks
grep -c "\[ \]" /hdd/GitHub/mb-worktree/dst/spec/dst/dst-implementation-roadmap.md

# Check overall progress
echo "Progress: $(grep -c '\[x\]' /hdd/GitHub/mb-worktree/dst/spec/dst/dst-implementation-roadmap.md) / $(grep -c '\[ \]' /hdd/GitHub/mb-worktree/dst/spec/dst/dst-implementation-roadmap.md) tasks complete"
```

---

**Next Action**: Begin Phase 1.1 - HttpResponse Header Support Verification
**Start With**: `nix develop --command grep -n "headers.*BTreeMap<String, String>" packages/web_server/src/lib.rs`
