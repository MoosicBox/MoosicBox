# HyperChad HTTP Events Example

This example demonstrates the complete HTTP request lifecycle event system in HyperChad, showcasing all 6 event types.

## Event Types

- **`fx-http-before-request`** - Shows loading spinner before request starts
- **`fx-http-after-request`** - Hides spinner after request completes (success or error)
- **`fx-http-success`** - Shows success message on successful response (2xx)
- **`fx-http-error`** - Shows error message on HTTP errors (4xx/5xx) or network failures
- **`fx-http-abort`** - Handles aborted requests
- **`fx-http-timeout`** - Shows timeout message when request takes too long

## Features Demonstrated

✅ Loading states with spinner visibility toggle  
✅ Success/error message display  
✅ Network error handling  
✅ Simulated slow/failing endpoints  
✅ Form submission with validation feedback  
✅ Multiple concurrent HTTP event handlers  
✅ Real-world task management UI  
✅ Clean HyperChad syntax with `fx` DSL

## Running the Example

### Development (with embedded assets):

```bash
cd packages/hyperchad/examples/http_events
PORT=3131 cargo run -- serve
```

Then open your browser to: **http://localhost:3131**

> **Note**: The default port is 8343. Set the `PORT` environment variable to use a different port.

### Production (expects external JS hosting):

```bash
PORT=3131 cargo run --no-default-features --features actix,vanilla-js -- serve
```

## Features

- `dev` - Enables embedded assets for local development (includes the bundled JavaScript)
- `assets` - Enables static asset serving
- `vanilla-js` - Enables vanilla JavaScript renderer with HTTP events plugin
- `actix` - Enables Actix web server backend

## Endpoints

- `GET /` - Main page with task form
- `POST /api/tasks` - Create a new task (500ms simulated delay)
- `POST /api/tasks/error` - Always returns 500 error
- `POST /api/tasks/slow` - Slow endpoint (3 second delay)

## Architecture

### Event Flow

```
Button Click
  ↓
fx-http-before-request: Show loading spinner
  ↓
HTTP Request via fetch()
  ↓
[Success Path]                    [Error Path]
  ↓                                   ↓
fx-http-after-request           fx-http-after-request
fx-http-success                 fx-http-error
  ↓                                   ↓
Hide spinner                    Hide spinner
Show success message            Show error message
```

### Fetch Interception

The `actions-http-events.ts` plugin wraps the global `fetch()` function to emit custom DOM events (`hyperchad:http-*`) at each lifecycle point. These events bubble up to elements with `fx-http-*` attributes.

## Code Highlights

### HyperChad `fx` DSL

```rust
button
    hx-post="/api/tasks"
    fx-http-before-request=fx {
        display("loading-spinner");
        no_display("success-message");
        log("Starting request...");
    }
    fx-http-success=fx {
        display("success-message");
        log("Success!");
    }
{ "Submit" }
```

### Clean Attribute Syntax

- Unquoted colors: `background=#3b82f6`
- Unquoted numbers: `padding=24`, `gap=12`
- Unquoted keywords: `type=button`, `visibility=hidden`
- Gap on containers: `gap=12` instead of `margin-bottom=12` on children
- Flexbox centering: `display=flex justify-content=center`

## Testing Different Scenarios

1. **Success**: Click "Add Task" → Shows loading → Success message
2. **Error**: Click "Test Error" → Shows loading → Error message
3. **Slow**: Click "Test Slow (3s)" → Shows loading for 3s → Success

## Developer Info

- Open browser console to see `log()` messages
- Check Network tab to see `fetch()` calls being intercepted
- All events emit as `hyperchad:http-*` custom DOM events
- The `actions-http-events.ts` plugin intercepts `window.fetch()`

## Implementation Details

This example uses:

- **HyperChad router** for page routing
- **Actix web server** backend (via hyperchad renderer)
- **Vanilla JS plugin** `actions-http-events.ts` for fetch interception
- **`fx` DSL** for declarative action syntax
- **Proper HyperChad conventions** (unquoted values, gap, etc.)
