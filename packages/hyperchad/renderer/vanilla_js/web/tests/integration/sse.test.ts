import { describe, expect, vi, beforeEach } from 'vitest';
import { sse, http, HttpResponse } from 'msw';
import { test } from '../helpers/test-extend';

describe('SSE', () => {
    beforeEach(() => {
        // Clear localStorage and cookies
        localStorage.clear();
        document.cookie.split(';').forEach((c) => {
            document.cookie = c
                .replace(/^ +/, '')
                .replace(
                    /=.*/,
                    '=;expires=' + new Date().toUTCString() + ';path=/',
                );
        });
        document.body.innerHTML = '';
        // Clean up window test globals
        Object.keys(window)
            .filter((k) => k.startsWith('__') && !k.startsWith('__vitest'))
            .forEach(
                (k) => delete (window as unknown as Record<string, unknown>)[k],
            );
    });

    describe('stream ID management', () => {
        test('generates and persists stream ID in localStorage', async ({
            worker,
        }) => {
            worker.use(
                sse('/$sse', () => {
                    // Keep connection open
                }),
            );

            await import('../../src/core');
            await import('../../src/uuid');
            const { initSSE } = await import('../../src/sse');
            initSSE();

            const streamId = localStorage.getItem('streamId');
            expect(streamId).toBeTruthy();
            expect(streamId).toMatch(/^[0-9a-f-]+$/i); // UUID format
        });

        test('reuses existing stream ID from localStorage', async ({
            worker,
        }) => {
            // Pre-set a stream ID
            localStorage.setItem('streamId', 'existing-id-123');

            worker.use(
                sse('/$sse', () => {
                    // Keep connection open
                }),
            );

            await import('../../src/core');
            await import('../../src/uuid');
            const { initSSE } = await import('../../src/sse');
            initSSE();

            const streamId = localStorage.getItem('streamId');
            expect(streamId).toBe('existing-id-123');
        });

        test('sets stream ID cookie', async ({ worker }) => {
            worker.use(
                sse('/$sse', () => {
                    // Keep connection open
                }),
            );

            await import('../../src/core');
            await import('../../src/uuid');
            const { initSSE } = await import('../../src/sse');
            initSSE();

            const cookie = document.cookie;
            expect(cookie).toContain('v-sse-stream-id=');
        });
    });

    describe('message handling', () => {
        test('view message triggers swapDom handler', async ({ worker }) => {
            const viewContent = '<div id="sse-content">Hello from SSE</div>';

            worker.use(
                sse('/$sse', ({ client }) => {
                    // Small delay to ensure listeners are set up
                    setTimeout(() => {
                        client.send({
                            event: 'view' as 'message',
                            data: viewContent,
                        });
                    }, 50);
                }),
            );

            const { on } = await import('../../src/core');

            const results: string[] = [];
            on('swapDom', (payload) => {
                results.push(payload.html as string);
            });
            (window as unknown as Record<string, unknown>).__swapDomResults =
                results;

            await import('../../src/uuid');
            const { initSSE } = await import('../../src/sse');
            initSSE();

            // Wait for message to be received and processed
            await vi.waitFor(
                () => {
                    expect(results.length).toBeGreaterThan(0);
                },
                { timeout: 5000 },
            );

            expect(results[0]).toContain('Hello from SSE');
        });

        test('partial_view message triggers swapHtml with style', async ({
            worker,
        }) => {
            // Test multi-line SSE data with style and HTML separated by blank line.
            // This verifies MSW correctly handles \n\n in SSE messages (fixed in v2.12.5).
            const styleContent = '<style>.test{color:red}</style>';
            const htmlContent = '<div>Updated Content</div>';
            const fullContent = `${styleContent}\n\n${htmlContent}`;

            worker.use(
                sse('/$sse', ({ client }) => {
                    setTimeout(() => {
                        client.send({
                            event: 'partial_view' as 'message',
                            id: 'target-element',
                            data: fullContent,
                        });
                    }, 50);
                }),
            );

            const { on } = await import('../../src/core');

            const swapHtmlResults: Array<{
                target: string;
                html: string;
                strategy: string;
            }> = [];

            const swapStyleResults: Array<{
                style: string;
                id: string;
            }> = [];

            on('swapHtml', (payload) => {
                swapHtmlResults.push({
                    target: payload.target as string,
                    html: payload.html as string,
                    strategy: payload.strategy,
                });
            });

            on('swapStyle', (payload) => {
                swapStyleResults.push({
                    style: payload.style as string,
                    id: payload.id as string,
                });
            });

            await import('../../src/uuid');
            const { initSSE } = await import('../../src/sse');
            initSSE();

            await vi.waitFor(
                () => {
                    expect(swapHtmlResults.length).toBeGreaterThan(0);
                    expect(swapStyleResults.length).toBeGreaterThan(0);
                },
                { timeout: 5000 },
            );

            // Verify the HTML was correctly split and received
            expect(swapHtmlResults[0].target).toBe('#target-element');
            expect(swapHtmlResults[0].html).toBe(htmlContent);
            expect(swapHtmlResults[0].strategy).toBe('this');

            // Verify the style was correctly split and received
            expect(swapStyleResults[0].style).toBe(styleContent);
            expect(swapStyleResults[0].id).toBe('target-element');
        });

        test('event message dispatches custom event', async ({ worker }) => {
            worker.use(
                sse('/$sse', ({ client }) => {
                    setTimeout(() => {
                        client.send({
                            event: 'event' as 'message',
                            data: 'myevent:payload-data',
                        });
                    }, 50);
                }),
            );

            await import('../../src/core');
            await import('../../src/uuid');
            await import('../../src/event');

            const results: string[] = [];
            window.addEventListener('v-myevent', ((e: CustomEvent) => {
                results.push(e.detail);
            }) as EventListener);

            const { initSSE } = await import('../../src/sse');
            initSSE();

            await vi.waitFor(
                () => {
                    expect(results.length).toBeGreaterThan(0);
                },
                { timeout: 5000 },
            );

            expect(results[0]).toBe('payload-data');
        });
    });

    describe('connection lifecycle', () => {
        test('handles connection errors gracefully', async ({ worker }) => {
            worker.use(
                sse('/$sse', ({ client }) => {
                    // Error the connection
                    client.error();
                }),
            );

            // Track console errors
            const errors: string[] = [];
            const originalError = console.error;
            console.error = (...args: unknown[]) => {
                errors.push(args.join(' '));
                originalError.apply(console, args);
            };

            await import('../../src/core');
            await import('../../src/uuid');
            const { initSSE } = await import('../../src/sse');
            initSSE();

            // Give time for at least one retry attempt
            await new Promise((resolve) => setTimeout(resolve, 2000));

            // Restore console.error
            console.error = originalError;

            // Should have logged SSE errors
            expect(
                errors.some(
                    (e: string) => e.includes('SSE') || e.includes('error'),
                ),
            ).toBe(true);
        });

        test('logs error on non-2xx response', async ({ worker }) => {
            worker.use(
                http.get('/$sse', () => {
                    return new HttpResponse(null, { status: 500 });
                }),
            );

            const errors: string[] = [];
            const originalError = console.error;
            console.error = (...args: unknown[]) => {
                errors.push(args.join(' '));
                originalError.apply(console, args);
            };

            await import('../../src/core');
            await import('../../src/uuid');
            const { initSSE } = await import('../../src/sse');
            initSSE();

            await new Promise((resolve) => setTimeout(resolve, 500));

            // Restore console.error
            console.error = originalError;

            expect(
                errors.some(
                    (e: string) => e.includes('500') || e.includes('SSE'),
                ),
            ).toBe(true);
        });
    });

    describe('multiple message types', () => {
        test('handles multiple sequential messages', async ({ worker }) => {
            worker.use(
                sse('/$sse', ({ client }) => {
                    setTimeout(() => {
                        client.send({
                            event: 'view' as 'message',
                            data: '<div>First</div>',
                        });
                    }, 50);
                    setTimeout(() => {
                        client.send({
                            event: 'view' as 'message',
                            data: '<div>Second</div>',
                        });
                    }, 100);
                    setTimeout(() => {
                        client.send({
                            event: 'view' as 'message',
                            data: '<div>Third</div>',
                        });
                    }, 150);
                }),
            );

            const { on } = await import('../../src/core');
            await import('../../src/uuid');

            const results: string[] = [];
            on('swapDom', (payload) => {
                results.push(payload.html as string);
            });

            const { initSSE } = await import('../../src/sse');
            initSSE();

            await vi.waitFor(
                () => {
                    expect(results.length).toBe(3);
                },
                { timeout: 5000 },
            );

            expect(results).toContain('<div>First</div>');
            expect(results).toContain('<div>Second</div>');
            expect(results).toContain('<div>Third</div>');
        });
    });
});
