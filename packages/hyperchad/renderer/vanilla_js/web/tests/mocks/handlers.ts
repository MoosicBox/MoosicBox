import { http, HttpResponse, sse } from 'msw';

export const handlers = [
    // Default SSE handler - keeps connection open
    sse('/$sse', () => {
        // No-op by default, tests override as needed
    }),

    // Default action handler
    http.post('/$action', () => {
        return new HttpResponse(null, { status: 204 });
    }),
];
