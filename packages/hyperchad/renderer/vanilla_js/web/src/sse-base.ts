import { v } from './core';
import {
    FetchEventSourceInit,
    fetchEventSource,
} from './vendored/fetch-event-source';

export const DEFAULT_SSE_STREAM_ID_STORAGE_KEY = 'streamId';
export const DEFAULT_SSE_STREAM_ID_COOKIE_NAME = 'v-sse-stream-id';

export interface EventSourceStreamOptions {
    streamIdStorageKey?: string;
    streamIdCookieName?: string;
    streamKey?: string;
    signal?: AbortSignal;
    includeSessionIdQuery?: boolean;
    onopen?: FetchEventSourceInit['onopen'];
    onmessage: NonNullable<FetchEventSourceInit['onmessage']>;
    onerror?: FetchEventSourceInit['onerror'];
}

type ActiveEventSourceStream = {
    streamId: string;
    controller: AbortController;
};

const activeEventSourceStreams = new Map<string, ActiveEventSourceStream>();

function resolveStreamKey(
    path: string,
    options: EventSourceStreamOptions,
): string {
    return options.streamKey ?? path;
}

export function hasActiveEventSourceStream(streamKey: string): boolean {
    const stream = activeEventSourceStreams.get(streamKey);
    return !!stream && !stream.controller.signal.aborted;
}

export function stopEventSourceStream(streamKey: string): boolean {
    const stream = activeEventSourceStreams.get(streamKey);
    if (!stream) {
        return false;
    }

    stream.controller.abort();
    activeEventSourceStreams.delete(streamKey);
    return true;
}

export function stopAllEventSourceStreams(): void {
    for (const stream of activeEventSourceStreams.values()) {
        stream.controller.abort();
    }

    activeEventSourceStreams.clear();
}

export function getOrCreateClientStreamId(
    streamIdStorageKey: string = DEFAULT_SSE_STREAM_ID_STORAGE_KEY,
): string {
    let id = localStorage.getItem(streamIdStorageKey);
    if (!id) {
        id = v.genUuid();
        localStorage.setItem(streamIdStorageKey, id);
    }
    return id;
}

export function setStreamIdCookie(cookieName: string, streamId: string): void {
    document.cookie = `${cookieName}=${streamId}; path=/; SameSite=Strict`;
}

export function createEventSourcePath(
    path: string,
    streamId: string,
    includeSessionIdQuery: boolean,
): string {
    if (!includeSessionIdQuery) {
        return path;
    }

    const url = new URL(path, window.location.origin);
    url.searchParams.set('session_id', streamId);
    return `${url.pathname}${url.search}`;
}

export function startEventSourceStream(
    path: string,
    options: EventSourceStreamOptions,
): string {
    const streamKey = resolveStreamKey(path, options);
    const existing = activeEventSourceStreams.get(streamKey);

    if (existing && !existing.controller.signal.aborted) {
        return existing.streamId;
    }

    if (existing) {
        activeEventSourceStreams.delete(streamKey);
    }

    const streamId = getOrCreateClientStreamId(options.streamIdStorageKey);
    const controller = new AbortController();

    if (options.signal) {
        if (options.signal.aborted) {
            controller.abort();
        } else {
            options.signal.addEventListener('abort', () => controller.abort(), {
                once: true,
            });
        }
    }

    setStreamIdCookie(
        options.streamIdCookieName ?? DEFAULT_SSE_STREAM_ID_COOKIE_NAME,
        streamId,
    );

    const streamPromise = fetchEventSource(
        createEventSourcePath(
            path,
            streamId,
            options.includeSessionIdQuery ?? false,
        ),
        {
            method: 'GET',
            signal: controller.signal,
            onopen:
                options.onopen ??
                (async (response: Response) => {
                    if (response.status >= 400) {
                        const status = response.status.toString();
                        console.error('Failed to open SSE', { status });
                    }
                }),
            onmessage: options.onmessage,
            onerror:
                options.onerror ??
                ((error) => {
                    if (error) {
                        if (typeof error === 'object' && 'message' in error) {
                            console.error('SSE error', error.message);
                        } else {
                            console.error('SSE error', error);
                        }
                    } else {
                        console.error('SSE error', error);
                    }
                }),
        },
    ).catch((error: unknown) => {
        if (!controller.signal.aborted) {
            console.error('SSE stream closed unexpectedly', error);
        }
    });

    activeEventSourceStreams.set(streamKey, {
        streamId,
        controller,
    });

    void streamPromise.finally(() => {
        const active = activeEventSourceStreams.get(streamKey);
        if (active && active.controller === controller) {
            activeEventSourceStreams.delete(streamKey);
        }
    });

    return streamId;
}
