# Basic Web Server Simulation Example

This example demonstrates how to use simvar with switchy_web_server to create a basic web server simulation.

## Overview

The simulation creates:

- **Web Server Host**: Serves HTTP requests on multiple endpoints
- **Multiple Client Actors**: Make periodic requests to test the server

## Features Demonstrated

- Setting up a web server using `switchy_web_server`
- Creating simulation hosts and clients with `simvar`
- Handling HTTP requests and responses
- Time-based simulation with configurable duration
- Error handling and logging
- CORS configuration
- JSON serialization/deserialization

## Endpoints

The web server provides these endpoints:

- `GET /api/v1/health` - Health check endpoint
- `GET /api/v1/status` - Server status with uptime information
- `POST /api/v1/echo` - Echo endpoint that accepts JSON and returns a response

## Running the Example

```bash
# From the MoosicBox root directory
cargo run -p simvar_basic_web_server_example

# Or with logging
RUST_LOG=debug cargo run -p simvar_basic_web_server_example
```

## Configuration

The simulation can be configured by modifying the `BasicWebServerBootstrap`:

- `server_port`: Port for the web server (default: 8080)
- `client_count`: Number of client actors (default: 3)
- `request_interval`: Time between client requests (default: 500ms)

## Expected Output

The simulation will show:

- Success/failure statistics
- Final simulation results

For detailed server startup and client request logs, run with `RUST_LOG=debug` or `RUST_LOG=info`.

## Key Concepts

### SimBootstrap

Configures the simulation parameters and sets up initial actors.

### Host Actor

Runs the web server that handles incoming HTTP requests.

### Client Actors

Make periodic HTTP requests to test server functionality.

### Simulation Time

Uses simvar's time simulation for deterministic testing.

## Next Steps

This basic example can be extended to:

- Add more complex request patterns
- Implement load testing scenarios
- Add database interactions
- Test error conditions and recovery
- Monitor performance metrics

See the other examples in this directory for more advanced scenarios.
