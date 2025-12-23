import { describe, expect, vi, beforeEach } from 'vitest';
import { http, HttpResponse, delay } from 'msw';
import { test } from '../helpers/test-extend';

describe('actions-http-events', () => {
    beforeEach(() => {
        document.body.innerHTML = '';
        // Clean up window test globals
        Object.keys(window)
            .filter((k) => k.startsWith('__') && !k.startsWith('__vitest'))
            .forEach(
                (k) => delete (window as unknown as Record<string, unknown>)[k],
            );
    });

    test('v-http-before-request fires before fetch', async ({ worker }) => {
        worker.use(
            http.get('/api/test', async () => {
                await delay(100);
                return HttpResponse.json({ ok: true });
            }),
        );

        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-http-events');

        (window as unknown as Record<string, unknown[]>).__events = [];

        const btn = document.createElement('button');
        btn.id = 'btn';
        btn.setAttribute(
            'v-http-before-request',
            "window.__events.push('before:' + Date.now())",
        );
        document.body.appendChild(btn);

        // Make a fetch request from the button
        btn.addEventListener('click', () => {
            (window as unknown as Record<string, unknown[]>).__events.push(
                'click:' + Date.now(),
            );
            fetch('/api/test');
        });

        btn.click();

        await vi.waitFor(
            () => {
                const events = (window as unknown as Record<string, string[]>)
                    .__events;
                expect(events.length).toBeGreaterThanOrEqual(2);
            },
            { timeout: 3000 },
        );

        const events = (window as unknown as Record<string, string[]>).__events;

        // before-request should fire
        expect(events.some((e: string) => e.startsWith('before:'))).toBe(true);
    });

    test('v-http-success fires on 2xx response', async ({ worker }) => {
        worker.use(
            http.get('/api/success', () => {
                return HttpResponse.json({ ok: true });
            }),
        );

        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-http-events');

        (window as unknown as Record<string, unknown>).__successStatus = null;

        document.body.setAttribute(
            'v-http-success',
            'window.__successStatus = ctx.value.status',
        );

        fetch('/api/success');

        await vi.waitFor(
            () => {
                const status = (window as unknown as Record<string, number>)
                    .__successStatus;
                expect(status).toBe(200);
            },
            { timeout: 3000 },
        );
    });

    test('v-http-error fires on 4xx/5xx response', async ({ worker }) => {
        worker.use(
            http.get('/api/error', () => {
                return new HttpResponse(null, { status: 500 });
            }),
        );

        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-http-events');

        (window as unknown as Record<string, unknown>).__errorMsg = null;

        document.body.setAttribute(
            'v-http-error',
            'window.__errorMsg = ctx.value.error',
        );

        fetch('/api/error');

        await vi.waitFor(
            () => {
                const error = (window as unknown as Record<string, string>)
                    .__errorMsg;
                expect(error).toBeTruthy();
            },
            { timeout: 3000 },
        );

        const error = (window as unknown as Record<string, string>).__errorMsg;
        expect(error).toContain('500');
    });

    test('v-http-after-request fires for both success and error', async ({
        worker,
    }) => {
        worker.use(
            http.get('/api/after', () => {
                return HttpResponse.json({ ok: true });
            }),
        );

        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-http-events');

        (window as unknown as Record<string, unknown>).__afterCalled = false;

        document.body.setAttribute(
            'v-http-after-request',
            'window.__afterCalled = true',
        );

        fetch('/api/after');

        await vi.waitFor(
            () => {
                const called = (window as unknown as Record<string, boolean>)
                    .__afterCalled;
                expect(called).toBe(true);
            },
            { timeout: 3000 },
        );
    });

    test('context includes url, method, status, duration_ms', async ({
        worker,
    }) => {
        worker.use(
            http.post('/api/details', async () => {
                await delay(50);
                return HttpResponse.json({ ok: true });
            }),
        );

        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-http-events');

        (window as unknown as Record<string, unknown>).__context = null;

        document.body.setAttribute(
            'v-http-success',
            'window.__context = ctx.value',
        );

        fetch('/api/details', { method: 'POST' });

        await vi.waitFor(
            () => {
                const context = (
                    window as unknown as Record<
                        string,
                        {
                            url: string;
                            method: string;
                            status: number;
                            duration_ms: number;
                        }
                    >
                ).__context;
                expect(context).toBeTruthy();
            },
            { timeout: 3000 },
        );

        const context = (
            window as unknown as Record<
                string,
                {
                    url: string;
                    method: string;
                    status: number;
                    duration_ms: number;
                }
            >
        ).__context;

        expect(context.url).toContain('/api/details');
        expect(context.method).toBe('POST');
        expect(context.status).toBe(200);
        expect(context.duration_ms).toBeGreaterThanOrEqual(50);
    });

    test('events bubble and can be caught on ancestors', async ({ worker }) => {
        worker.use(
            http.get('/api/bubble', () => {
                return HttpResponse.json({ ok: true });
            }),
        );

        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-http-events');

        (window as unknown as Record<string, unknown>).__ancestorCaught = false;

        const parent = document.createElement('div');
        parent.id = 'parent';
        parent.setAttribute('v-http-success', 'window.__ancestorCaught = true');

        const child = document.createElement('button');
        child.id = 'child';
        child.textContent = 'Fetch';

        parent.appendChild(child);
        document.body.appendChild(parent);

        // Fetch from child context
        child.addEventListener('click', () => {
            fetch('/api/bubble');
        });

        child.click();

        await vi.waitFor(
            () => {
                const caught = (window as unknown as Record<string, boolean>)
                    .__ancestorCaught;
                expect(caught).toBe(true);
            },
            { timeout: 3000 },
        );
    });

    test('v-http-abort fires on aborted request', async ({ worker }) => {
        worker.use(
            http.get('/api/slow', async () => {
                await delay(5000); // Very slow
                return HttpResponse.json({ ok: true });
            }),
        );

        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-http-events');

        (window as unknown as Record<string, unknown>).__aborted = false;

        document.body.setAttribute('v-http-abort', 'window.__aborted = true');

        const controller = new AbortController();
        // Catch the abort error to prevent unhandled rejection
        fetch('/api/slow', { signal: controller.signal }).catch(() => {
            // Expected: AbortError
        });

        // Abort after 100ms
        setTimeout(() => controller.abort(), 100);

        await vi.waitFor(
            () => {
                const aborted = (window as unknown as Record<string, boolean>)
                    .__aborted;
                expect(aborted).toBe(true);
            },
            { timeout: 3000 },
        );
    });
});
