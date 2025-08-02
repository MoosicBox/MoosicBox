# API Testing Simulation Example

This example demonstrates comprehensive API testing using simvar and moosicbox_web_server to validate REST endpoint behavior.

## Overview

The simulation creates a complete API testing environment with:
- **REST API Server**: Full CRUD operations for user management
- **Test Scenarios**: Happy path, error handling, edge cases, and concurrency testing
- **Comprehensive Validation**: HTTP status codes, response formats, and error conditions
- **Detailed Reporting**: Test results with timing and error analysis

## Test Scenarios

### Happy Path Testing
Tests successful operations under normal conditions:
- Create new users with valid data
- Retrieve users by ID
- List all users
- Update user information
- Delete users

### Error Handling Testing
Validates proper error responses:
- Request non-existent resources (404 errors)
- Send invalid request data (400 errors)
- Test malformed JSON payloads
- Verify error message formats

### Edge Case Testing
Tests boundary conditions and unusual inputs:
- Very long field values
- Empty or null fields
- Special characters in data
- Large payload sizes
- Unicode handling

### Concurrency Testing
Validates behavior under concurrent access:
- Multiple simultaneous user creation
- Concurrent read/write operations
- Race condition detection
- Data consistency verification

## API Endpoints

The server implements a complete REST API:

- `POST /api/v1/users` - Create a new user
- `GET /api/v1/users` - List all users
- `GET /api/v1/users/{id}` - Get user by ID
- `PUT /api/v1/users/{id}` - Update user
- `DELETE /api/v1/users/{id}` - Delete user

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

### Update User Request
```json
{
  "name": "Updated Name",
  "email": "updated@example.com"
}
```

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
Tests: 12 (Passed: 10, Failed: 2)

Test Details:
  [PASS] HappyPath - create_user (45ms)
  [PASS] HappyPath - get_user_by_id (23ms)
  [PASS] HappyPath - list_users (18ms)
  [PASS] ErrorHandling - get_non_existent_user (15ms)
  [FAIL] ErrorHandling - create_user_invalid_data (12ms) - Error: Expected 400, got: 500
  [PASS] EdgeCases - create_user_long_name (67ms)
  [PASS] Concurrency - concurrent_create_user_0 (34ms)
  ...
```

## Key Features

### Comprehensive Test Coverage
- Multiple test scenarios covering different aspects
- Both positive and negative test cases
- Boundary condition testing
- Concurrent operation validation

### Detailed Result Tracking
- Individual test pass/fail status
- Response time measurement
- Error message capture
- Categorized test reporting

### Realistic API Implementation
- Full CRUD operations
- Proper HTTP status codes
- JSON request/response handling
- In-memory data persistence

### Deterministic Testing
- Controlled simulation environment
- Reproducible test results
- Configurable test duration
- Ordered test execution

## Use Cases

This example is ideal for:
- **API Contract Testing**: Verify endpoint behavior matches specifications
- **Regression Testing**: Ensure API changes don't break existing functionality
- **Integration Testing**: Test API interactions with clients
- **Performance Testing**: Measure API response times
- **Error Handling Validation**: Ensure proper error responses
- **Concurrency Testing**: Verify thread-safe operations

## Advanced Scenarios

Extend this example to test:
- Authentication and authorization
- Rate limiting and throttling
- Database transaction handling
- Caching behavior
- API versioning
- Content negotiation
- File upload/download
- WebSocket connections

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
- **Happy Path**: Should have 100% pass rate
- **Error Handling**: Should properly return expected error codes
- **Edge Cases**: May have mixed results depending on validation rules
- **Concurrency**: Should maintain data consistency

## Integration with CI/CD

This simulation can be integrated into continuous integration pipelines:
- Run as part of automated testing
- Generate test reports in standard formats
- Set pass/fail thresholds for deployment gates
- Monitor API performance over time