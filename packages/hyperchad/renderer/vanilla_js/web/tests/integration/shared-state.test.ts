import { beforeEach, describe, expect, vi } from 'vitest';
import { http, HttpResponse, sse } from 'msw';
import { test } from '../helpers/test-extend';

describe('shared-state plugin', () => {
    beforeEach(() => {
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

        Object.keys(window)
            .filter((k) => k.startsWith('__') && !k.startsWith('__vitest'))
            .forEach(
                (k) => delete (window as unknown as Record<string, unknown>)[k],
            );
    });

    test('subscribes, dispatches events, and unsubscribes removed channels', async ({
        worker,
    }) => {
        document.body.innerHTML = `
            <div id="app" data-shared-state-channel="room:alpha"></div>
        `;

        const outboundMessages: unknown[] = [];
        const sessions = {
            post: '',
            sse: '',
        };

        worker.use(
            sse('/$shared-state/transport/sse', ({ request, client }) => {
                sessions.sse =
                    new URL(request.url).searchParams.get('session_id') ?? '';

                setTimeout(() => {
                    client.send({
                        data: JSON.stringify({
                            Event: {
                                channel_id: 'room:alpha',
                                revision: 2,
                            },
                        }),
                        event: 'message',
                    });
                }, 50);
            }),
            http.post('/$shared-state/transport', async ({ request }) => {
                sessions.post =
                    new URL(request.url).searchParams.get('session_id') ?? '';
                outboundMessages.push(await request.json());

                return new HttpResponse(null, { status: 204 });
            }),
        );

        const core = await import('../../src/core');
        await import('../../src/uuid');

        const inboundEventDetails: string[] = [];
        window.addEventListener('v-shared-state-event', (event) => {
            inboundEventDetails.push((event as CustomEvent<string>).detail);
        });

        await import('../../src/sse');
        await import('../../src/shared-state');

        core.triggerHandlers('domLoad', {
            elements: [document.documentElement],
            initial: false,
            navigation: false,
        });

        await vi.waitFor(
            () => {
                expect(outboundMessages.length).toBeGreaterThan(0);
            },
            { timeout: 5000 },
        );

        expect(outboundMessages[0]).toEqual({
            Subscribe: {
                channel_id: 'room:alpha',
                last_seen_revision: null,
            },
        });
        expect(sessions.sse).toBeTruthy();
        expect(sessions.post).toBe(sessions.sse);

        await vi.waitFor(
            () => {
                expect(inboundEventDetails.length).toBeGreaterThan(0);
            },
            { timeout: 5000 },
        );

        expect(JSON.parse(inboundEventDetails[0])).toEqual({
            channel_id: 'room:alpha',
            revision: 2,
        });

        document.body.innerHTML = `<div id="app"></div>`;
        core.triggerHandlers('domLoad', {
            elements: [document.documentElement],
            initial: false,
            navigation: false,
        });

        await vi.waitFor(
            () => {
                expect(
                    outboundMessages.some(
                        (message) =>
                            JSON.stringify(message) ===
                            JSON.stringify({
                                Unsubscribe: {
                                    channel_id: 'room:alpha',
                                },
                            }),
                    ),
                ).toBe(true);
            },
            { timeout: 5000 },
        );
    });
});
