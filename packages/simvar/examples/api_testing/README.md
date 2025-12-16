# API Testing Simulation Example

This example demonstrates API testing using simvar and switchy_web_server to validate REST endpoint behavior.

## Overview

The simulation creates an API testing environment with:

- **REST API Server**: Simplified CRUD endpoints for user management
- **Test Scenarios**: Happy path, error handling, and edge case testing
- **HTTP Validation**: Status code verification and response timing
- **Detailed Reporting**: Test results with timing and error analysis

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

```bash
# From the MoosicBox root directory
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

## Key Features

### Test Coverage

- Multiple test scenarios: happy path, error handling, edge cases
- Both positive and negative test cases
- Boundary condition testing
- **Planned**: Full concurrent operation validation

### Detailed Result Tracking

- Individual test pass/fail status
- Response time measurement
- Error message capture
- Categorized test reporting

### Simplified API Implementation

- Basic CRUD endpoint structure
- HTTP status code responses
- JSON response handling
- In-memory data persistence
- **Limitation**: Simplified implementations for demonstration (hardcoded data, placeholder operations)

### Deterministic Testing

- Controlled simulation environment
- Reproducible test results
- Configurable test duration (20 seconds by default)
- Ordered test execution (random order disabled)

## Use Cases

This example demonstrates:

- **Simulation Testing**: Using simvar to test HTTP APIs in a controlled environment
- **Basic API Testing Patterns**: Structure for organizing and executing API tests
- **Response Validation**: Checking HTTP status codes and response timing
- **Test Result Tracking**: Collecting and reporting test outcomes

**Potential Extensions** (not currently implemented):

- Full API contract testing with request/response validation
- Regression testing with complete CRUD implementations
- Performance testing under load
- Comprehensive concurrency testing

## Potential Extensions

This example could be extended to test:

- **Request Body Parsing**: Parse and validate JSON request bodies
- **Path Parameter Extraction**: Extract and use URL path parameters
- **Complete CRUD Operations**: Implement actual update and delete functionality
- **Concurrency Testing**: Full implementation of concurrent API request scenarios
- **Authentication and Authorization**: Add authentication testing
- **Rate Limiting**: Test API rate limiting behavior
- **Database Integration**: Replace in-memory storage with actual database operations
- **Caching**: Test caching behavior
- **API Versioning**: Test multiple API versions

## Test Result Interpretation

### Response Times

- **Fast operations** (&lt;50ms): Simple CRUD operations
- **Medium operations** (50-200ms): Complex queries or validations
- **Slow operations** (&gt;200ms): May indicate performance issues

### Error Rates

- **0% errors**: All tests passing (ideal)
- **&lt;5% errors**: Acceptable for non-critical issues
- **&gt;5% errors**: Indicates significant problems requiring investigation

### Test Categories

- **Happy Path**: Tests basic successful operations (create, get, list)
- **Error Handling**: Verifies expected error status codes (note: some may fail due to simplified implementation)
- **Edge Cases**: Tests boundary conditions (accepts either success or validation errors)
- **Concurrency**: Currently simplified and not performing full tests

## Integration with CI/CD

This simulation can be used in continuous integration pipelines:

- Run as part of automated testing (exit code reflects test results)
- Parse output for test result summaries
- Use as a template for building more comprehensive API test suites

**Planned**: Generate test reports in standard formats (JUnit, TAP, etc.)
