# MoosicBox HyperChad VanillaJS Web Library

A client-side JavaScript/TypeScript library that provides the browser runtime for HyperChad applications. This package contains the core client-side functionality that enables HyperChad's reactive UI system to work in web browsers.

## Overview

The HyperChad VanillaJS Web Library is the client-side counterpart to the HyperChad renderer system. It provides:

- **Event-driven DOM management** with custom event handling
- **HTMX-style routing** with declarative HTTP requests
- **Action system** for server communication
- **Server-sent events (SSE)** for real-time updates
- **Client-side navigation** with caching and prefetching
- **Form handling** with automatic serialization
- **Canvas integration** for graphics rendering
- **Tauri integration** for desktop applications

## Features

### Core Event System

- Custom event handling with typed payloads
- DOM lifecycle management (load, swap, style updates)
- Element and attribute observers
- Message passing between components

### HTTP Integration

- HTMX-compatible request handling
- Declarative routing with `hx-*` attributes
- Automatic header management
- Fragment-based partial updates

### Action System

- Client-server action communication
- JavaScript evaluation in controlled contexts
- Style manipulation utilities
- Throttling and debouncing support

### Real-time Communication

- Server-sent events with automatic reconnection
- Stream ID management with localStorage
- Error handling and logging
- Cookie-based session management

### Navigation

- Client-side routing with history management
- Document caching and prefetching
- Link interception and handling
- Fallback to server navigation

### Form Handling

- Automatic form serialization
- File upload support
- Validation integration
- Submit event handling

## Installation

```bash
npm install @hyperchad/vanilla-js
# or
pnpm add @hyperchad/vanilla-js
# or
yarn add @hyperchad/vanilla-js
```

## Usage

### Basic Setup

```typescript
import { on, onElement, onAttr } from '@hyperchad/vanilla-js';

// Listen for DOM load events
on('domLoad', ({ initial, navigation, elements }) => {
    console.log('DOM loaded', { initial, navigation });
    elements.forEach((element) => {
        // Process loaded elements
    });
});

// Handle element lifecycle
onElement(({ element }) => {
    console.log('Element processed:', element);
});

// Watch for specific attributes
onAttr('data-component', ({ element, attr }) => {
    console.log('Component attribute found:', attr);
});
```

### HTMX-Style Routing

```html
<!-- Declarative HTTP requests -->
<button hx-get="/api/data" hx-swap="innerHTML">Load Data</button>
<button hx-post="/api/submit" hx-trigger="click">Submit</button>
<div hx-get="/api/content" hx-trigger="load"></div>
```

```typescript
import { processRoute } from '@hyperchad/vanilla-js';

// Programmatic route processing
const button = document.querySelector('button');
processRoute(button, {
    headers: { Authorization: 'Bearer token' },
});
```

### Action System

```typescript
import { triggerAction, evaluate } from '@hyperchad/vanilla-js';

// Trigger server actions
triggerAction({
    action: 'updateUser',
    value: { name: 'John', email: 'john@example.com' },
});

// Evaluate JavaScript with context
const result = evaluate('element.textContent = value', {
    element: document.querySelector('#target'),
    value: 'Hello World',
});
```

### Server-Sent Events

```typescript
import { onMessage } from '@hyperchad/vanilla-js';

// Listen for specific message types
onMessage('notification', (data, id) => {
    console.log('Notification received:', data, id);
});

onMessage('update', (data) => {
    const payload = JSON.parse(data);
    // Handle real-time updates
});
```

### Navigation

```typescript
import { navigate } from '@hyperchad/vanilla-js';

// Programmatic navigation
navigate('/dashboard');

// Navigation is automatically set up for links with proper attributes
```

```html
<!-- Automatic link handling -->
<a href="/page" data-nav>Navigate to Page</a>
```

### Form Handling

```html
<!-- Forms are automatically handled -->
<form hx-post="/api/submit">
    <input name="username" type="text" />
    <input name="password" type="password" />
    <button type="submit">Submit</button>
</form>
```

### Canvas Integration

```typescript
import { setupCanvas } from '@hyperchad/vanilla-js';

// Canvas elements are automatically processed
const canvas = document.querySelector('canvas[data-canvas]');
// Canvas functionality is set up automatically
```

### Tauri Integration

```typescript
import { setupTauriEvents } from '@hyperchad/vanilla-js';

// Tauri event handling for desktop applications
// Events are automatically bridged between Tauri and web contexts
```

## Event Types

### Core Events

```typescript
// DOM lifecycle events
on('domLoad', ({ initial, navigation, elements }) => {
    // Handle DOM load
});

on('swapDom', ({ html, url }) => {
    // Handle DOM replacement
});

on('swapHtml', ({ target, html, inner }) => {
    // Handle HTML swapping
});

on('swapStyle', ({ id, style }) => {
    // Handle style updates
});
```

### Custom Events

```typescript
// Element-specific handlers
onElement(({ element }) => {
    // Process any element
});

onAttr('custom-attr', ({ element, attr }) => {
    // Handle specific attributes
});

onAttrValue('data-type', 'modal', ({ element, attr }) => {
    // Handle specific attribute values
});
```

## API Reference

### Core Functions

- `on<T>(event: T, handler: Handler<T>)` - Register event handler
- `onElement(handler: ElementHandler)` - Register element handler
- `onAttr(attr: string, handler: AttrHandler)` - Register attribute handler
- `onAttrValue(attr: string, value: string, handler: AttrHandler)` - Register attribute value handler
- `onMessage(type: string, handler: MessageHandler)` - Register message handler
- `triggerHandlers<T>(event: T, payload: EventPayloads[T])` - Trigger event handlers
- `triggerMessage(type: string, data: string, id?: string)` - Trigger message handlers

### Routing Functions

- `processRoute(element: HTMLElement, options?: RequestInit)` - Process route for element
- `handleResponse(element: HTMLElement, text: string)` - Handle HTTP response

### Action Functions

- `triggerAction(action: { action: unknown; value?: unknown })` - Trigger server action
- `evaluate<T>(script: string, context: Record<string, unknown>)` - Evaluate JavaScript with context

### Navigation Functions

- `navigate(url: string)` - Navigate to URL with caching
- `setupLinkHandlers()` - Set up automatic link handling

### Utility Functions

- `splitHtml(html: string)` - Split HTML and style content
- `decodeHtml(html: string)` - Decode HTML entities
- `processElement(element: HTMLElement)` - Process element and children
- `handleError<T>(type: string, func: () => T)` - Error handling wrapper

## Configuration

### TypeScript Configuration

The package includes a comprehensive TypeScript configuration:

```json
{
    "compilerOptions": {
        "target": "ESNext",
        "module": "ESNext",
        "moduleResolution": "node",
        "strict": true,
        "lib": ["ESNext.Array", "DOM", "ESNext", "DOM.iterable"]
    }
}
```

### Build Scripts

```json
{
    "scripts": {
        "build": "tsc",
        "typecheck": "tsc --noEmit",
        "lint": "eslint src",
        "lint:write": "eslint src --fix",
        "pretty": "prettier --check \"**/*.{js,ts,tsx}\"",
        "pretty:write": "prettier --write \"**/*.{js,ts,tsx}\"",
        "validate": "pnpm typecheck && pnpm lint && pnpm pretty"
    }
}
```

## Architecture

### Event-Driven Design

The library uses an event-driven architecture where:

1. **DOM events** trigger application logic
2. **HTTP responses** update the DOM declaratively
3. **Server actions** provide bidirectional communication
4. **SSE messages** enable real-time updates
5. **Navigation events** manage client-side routing

### Integration Points

- **HyperChad Renderer**: Provides the HTML/CSS that this library enhances
- **Server Components**: Communicates via HTTP and SSE
- **Tauri Desktop**: Bridges web and native functionality
- **Canvas Graphics**: Provides interactive graphics capabilities

### Performance Features

- **Request caching** for navigation
- **Throttling** for action evaluation
- **Lazy loading** for route prefetching
- **Efficient DOM updates** with targeted swapping

## Browser Compatibility

- **Modern Browsers**: Chrome 80+, Firefox 75+, Safari 13+, Edge 80+
- **ES Modules**: Native ES module support required
- **Fetch API**: Built-in fetch support required
- **EventSource**: Server-sent events support required

## Development

### Building

```bash
pnpm build
```

### Type Checking

```bash
pnpm typecheck
```

### Linting

```bash
pnpm lint
pnpm lint:write  # Auto-fix issues
```

### Code Formatting

```bash
pnpm pretty
pnpm pretty:write  # Auto-format code
```

### Full Validation

```bash
pnpm validate  # Run all checks
```

## Integration with HyperChad

This library is designed to work seamlessly with the HyperChad ecosystem:

1. **Server-side rendering** generates the initial HTML
2. **Client-side hydration** adds interactivity
3. **Partial updates** maintain state during navigation
4. **Real-time sync** keeps multiple clients in sync
5. **Action handling** provides server communication

## Contributing

This package is part of the MoosicBox project. See the main repository for contribution guidelines.

## License

Licensed under the ISC License. See the main MoosicBox repository for details.
