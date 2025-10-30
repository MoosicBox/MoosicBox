# API Testing Simulation Example

This example demonstrates comprehensive REST API endpoint testing using simvar and moosicbox_web_server in a deterministic simulation environment.

## Summary

This simulation creates a complete API testing framework that validates REST endpoint behavior through multiple test scenarios including happy path operations, error handling, edge cases, and concurrency testing. The example showcases how to use simvar for reproducible API contract testing with detailed result tracking and reporting.

## What This Example Demonstrates

- Setting up a REST API server with CRUD endpoints using moosicbox_web_server
- Creating test client actors that validate API contracts
- Running multiple test scenarios (happy path, error handling, edge cases, concurrency)
- Tracking detailed test results with pass/fail status and timing metrics
- Using simulation time for deterministic, reproducible testing
- Validating HTTP status codes and response formats
- Organizing and reporting comprehensive test results

## Prerequisites

- Rust toolchain (see `rust-toolchain.toml` in project root)
- Basic understanding of REST APIs and HTTP methods
- Familiarity with async Rust and the tokio runtime
- Understanding of CRUD operations (Create, Read, Update, Delete)

## Test Scenarios

### Happy Path Testing

Tests successful operations under normal conditions:

- Create new users (simplified implementation with hardcoded data)
- Retrieve users by ID
- List all users

**Note**: Update and delete operations are implemented as placeholder endpoints that return success messages without performing actual operations.

### Error Handling Testing

Validates error response status codes:

- Request non-existent resources (404 errors)
- Send requests with invalid JSON structure (expects 400 errors)

**Note**: The current implementation does not parse or validate request body data for POST requests.

### Edge Case Testing

Tests boundary conditions:

- Very long field values (1000+ character strings)
- Accepts either successful creation or validation error responses

### Concurrency Testing

**Planned**: Concurrency testing is currently a placeholder and not fully implemented in this example.

## API Endpoints

The server implements these REST endpoints:

- `POST /api/v1/users` - Create a new user (simplified: uses hardcoded data, does not parse request body)
- `GET /api/v1/users` - List all users
- `GET /api/v1/users/{id}` - Get user by ID (simplified: returns first user, does not extract path parameter)
- `PUT /api/v1/users/{id}` - Update user (placeholder: returns success message without updating)
- `DELETE /api/v1/users/{id}` - Delete user (placeholder: returns success message without deleting)

**Implementation Note**: This is a simplified demonstration. Path parameter extraction, request body parsing, and actual update/delete operations are marked as "in a real implementation" in the code.

## Data Models

### User

```json
{
    "id": "uuid-string",
    "name": "User Name",
    "email": "user@example.com",
    "created_at": 1234567890
}
```

### Create User Request

```json
{
    "name": "New User",
    "email": "newuser@example.com"
}
```

**Note**: The current implementation does not parse this request body. The POST endpoint creates users with hardcoded values.

## Running the Example

From the MoosicBox root directory:

```bash
# Basic run
cargo run --manifest-path packages/simvar/examples/api_testing/Cargo.toml

# Or using the package name
cargo run -p simvar_api_testing_example

# With detailed logging
RUST_LOG=info cargo run -p simvar_api_testing_example

# With debug logging for troubleshooting
RUST_LOG=debug cargo run -p simvar_api_testing_example
```

## Configuration

Customize the testing by modifying `ApiTestingBootstrap`:

```rust
ApiTestingBootstrap {
    server_port: 8082,
    test_scenarios: vec![
        TestScenario::HappyPath,
        TestScenario::ErrorHandling,
        TestScenario::EdgeCases,
        TestScenario::Concurrency,
    ],
}
```

## Expected Output

The simulation provides:

- Real-time test progress updates
- Individual test results (PASS/FAIL)
- Response time measurements
- Error details for failed tests
- Final test summary with statistics

Example output:

```
=== API TESTING RESULTS ===
Tests: 7 (Passed: 5, Failed: 2)

Test Details:
  [PASS] HappyPath - create_user (45ms)
  [PASS] HappyPath - get_user_by_id (23ms)
  [PASS] HappyPath - list_users (18ms)
  [PASS] ErrorHandling - get_non_existent_user (15ms)
  [FAIL] ErrorHandling - create_user_invalid_data (12ms) - Error: Expected 400, got: 201
  [PASS] EdgeCases - create_user_long_name (67ms)
  ...
```

**Note**: The test count reflects the actual implemented tests. Concurrency tests are currently simplified and do not perform actual API requests.

## Code Walkthrough

### 1. Main Simulation Entry Point

The `main()` function initializes and runs the simulation:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bootstrap = ApiTestingBootstrap::new();
    let results = run_simulation(bootstrap)?;

    // Print test results summary
    println!("\n=== API TESTING RESULTS ===");
    for result in &results {
        println!("{result}");
    }

    Ok(())
}
```

### 2. Bootstrap Configuration

`ApiTestingBootstrap` configures the simulation parameters:

```rust
struct ApiTestingBootstrap {
    server_port: u16,
    test_scenarios: Vec<TestScenario>,
    test_results: Arc<Mutex<TestResults>>,
}
```

The `build_sim()` method sets simulation duration:

```rust
fn build_sim(&self, config: SimConfig) -> SimConfig {
    config.with_duration(Duration::from_secs(20))
}
```

### 3. Server Host Setup

The `on_start()` method creates the API server host:

```rust
sim.host("api-server", || {
    Box::pin(async move {
        let builder = WebServerBuilder::default()
            .with_port(self.server_port)
            .with_scope(create_api_routes());

        builder.serve().await?;
        Ok(())
    })
});
```

### 4. Test Client Actors

Test clients are spawned for each scenario:

```rust
sim.client("happy-path-tester", async move {
    // Create user
    let response = client.request(Method::Post, &create_url).send().await?;

    // Validate response
    assert_eq!(response.status(), StatusCode::CREATED);

    // Record test result
    results.lock().unwrap().record_test(/* ... */);

    Ok(())
});
```

### 5. Test Result Tracking

Results are collected using a thread-safe structure:

```rust
struct TestResults {
    tests: Vec<TestResult>,
    total: u32,
    passed: u32,
    failed: u32,
}
```

## Key Concepts

### Deterministic Testing with Simvar

Simvar provides reproducible test execution:

- **Controlled time**: Tests run in simulation time, not real time
- **Reproducible results**: Same test runs produce identical outcomes
- **No external dependencies**: All components run within the simulation

### Test Scenario Organization

Tests are organized by category:

- **Happy Path**: Validates successful operations under normal conditions
- **Error Handling**: Ensures proper error responses for invalid requests
- **Edge Cases**: Tests boundary conditions and unusual inputs
- **Concurrency**: Validates behavior under concurrent access

### API Contract Validation

The example demonstrates:

- **HTTP method validation**: Correct methods for each operation (GET, POST, PUT, DELETE)
- **Status code validation**: Expected status codes for success and error cases
- **Response format validation**: JSON structure and content verification
- **Timing measurements**: Response time tracking for performance analysis

## Testing the Example

### Running the Simulation

Execute the example and observe the test execution:

```bash
cargo run -p simvar_api_testing_example
```

### Interpreting Results

The simulation outputs test results in the following format:

```
=== API TESTING RESULTS ===
Tests: 7 (Passed: 5, Failed: 2)

Test Details:
  [PASS] HappyPath - create_user (45ms)
  [PASS] HappyPath - get_user_by_id (23ms)
  [PASS] HappyPath - list_users (18ms)
  [PASS] ErrorHandling - get_non_existent_user (15ms)
  [FAIL] ErrorHandling - create_user_invalid_data (12ms) - Error: Expected 400, got: 201
  [PASS] EdgeCases - create_user_long_name (67ms)
```

### Success Criteria

- **100% pass rate**: All tests should pass in a complete implementation
- **Fast response times**: Most operations should complete in <100ms
- **Correct status codes**: Each endpoint returns appropriate HTTP status codes
- **No simulation errors**: The simulation completes without panics or timeouts

## Troubleshooting

### Simulation Doesn't Start

**Problem**: Example fails to compile or run

**Solutions**:

- Verify Rust toolchain is up to date: `rustup update`
- Check dependencies: `cargo check -p simvar_api_testing_example`
- Enable debug logging: `RUST_LOG=debug cargo run -p simvar_api_testing_example`

### Port Already in Use

**Problem**: Error about port 8082 already being in use

**Solutions**:

- Change the port in `ApiTestingBootstrap::new()` to an available port
- Check for other processes using the port: `lsof -i :8082` (Linux/macOS)
- Kill the conflicting process or use a different port

### Tests Failing

**Problem**: Some tests show FAIL status

**Solutions**:

- Review the error message for each failed test
- Check if the API implementation matches expected behavior
- Verify HTTP status codes are correct for each endpoint
- Note: Some failures are expected in the simplified demonstration implementation

### Slow Performance

**Problem**: Simulation takes too long to complete

**Solutions**:

- Reduce the simulation duration in `build_sim()`
- Decrease the number of test scenarios
- Check system resources and close other applications

## Related Examples

- **basic_web_server**: Simpler web server example demonstrating fundamental concepts
- **Other simvar examples**: Check `packages/simvar/examples/` for more simulation patterns
