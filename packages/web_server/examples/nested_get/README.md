# Web Server Nested GET Example

Shows how to create a web server with a route nested under a scope.

## What it does

- Creates a web server with CORS configuration
- Creates a scope with prefix "/nested"
- Adds a GET route at "/example" to that scope
- Results in the endpoint being at "/nested/example"
