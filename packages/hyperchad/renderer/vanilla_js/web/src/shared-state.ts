import { on } from './core';
import { hasActiveEventSourceStream, startEventSourceStream } from './sse-base';

const SHARED_STATE_CHANNEL_ATTR = 'data-shared-state-channel';
const SHARED_STATE_SESSION_STORAGE_KEY = 'sharedStateSessionId';
const SHARED_STATE_SESSION_COOKIE_NAME = 'v-shared-state-session-id';
const SHARED_STATE_TRANSPORT_POST_PATH = '/$shared-state/transport';
const SHARED_STATE_TRANSPORT_SSE_PATH = '/$shared-state/transport/sse';
const SHARED_STATE_STREAM_KEY = '/$shared-state/transport/sse';

type SharedStateTransportSubscribe = {
    channel_id: string;
    last_seen_revision: number | null;
};

type SharedStateTransportUnsubscribe = {
    channel_id: string;
};

type SharedStateTransportPing = {
    sent_at_ms: number;
};

type SharedStateTransportOutbound =
    | { Subscribe: SharedStateTransportSubscribe }
    | { Unsubscribe: SharedStateTransportUnsubscribe }
    | { Ping: SharedStateTransportPing };

type SharedStateSnapshotEnvelope = {
    channel_id: string;
    revision: number;
};

type SharedStateEventEnvelope = {
    channel_id: string;
    revision: number;
};

type SharedStateTransportInbound =
    | { Snapshot: SharedStateSnapshotEnvelope }
    | { Event: SharedStateEventEnvelope }
    | {
          CommandAccepted: {
              command_id: string;
              resulting_revision: number;
          };
      }
    | {
          CommandRejected: {
              command_id: string;
              reason: string;
          };
      }
    | { Pong: SharedStateTransportPing };

const desiredChannels = new Set<string>();
const subscribedChannels = new Set<string>();
const lastSeenRevisionByChannel = new Map<string, number>();

let sharedStateSessionId: string | null = null;
let sharedStateConnected = false;

function asRecord(value: unknown): Record<string, unknown> | null {
    if (typeof value !== 'object' || value === null || Array.isArray(value)) {
        return null;
    }

    return value as Record<string, unknown>;
}

function parseChannelList(value: string | null): string[] {
    if (!value) {
        return [];
    }

    return value
        .split(',')
        .map((entry) => entry.trim())
        .filter((entry) => entry.length > 0);
}

function getDesiredChannelsFromDocument(): Set<string> {
    const channels = new Set<string>();

    const elements = document.querySelectorAll<HTMLElement>(
        `[${SHARED_STATE_CHANNEL_ATTR}]`,
    );

    elements.forEach((element) => {
        parseChannelList(
            element.getAttribute(SHARED_STATE_CHANNEL_ATTR),
        ).forEach((channel) => channels.add(channel));
    });

    return channels;
}

function dispatchSharedStateEvent(eventName: string, detail: string): void {
    window.dispatchEvent(
        new CustomEvent(`v-${eventName}`, {
            detail,
        }),
    );
}

function extractInboundVariant(
    payload: unknown,
): [keyof SharedStateTransportInbound, unknown] | null {
    const record = asRecord(payload);
    if (!record) {
        return null;
    }

    const entries = Object.entries(record);
    if (entries.length !== 1) {
        return null;
    }

    const [variant, value] = entries[0];
    return [variant as keyof SharedStateTransportInbound, value];
}

function toSerializedPayload(payload: unknown): string {
    if (typeof payload === 'string') {
        return payload;
    }

    try {
        return JSON.stringify(payload);
    } catch (error) {
        console.error('Failed to serialize shared-state payload', error);
        return '';
    }
}

function updateLastSeenRevision(payload: unknown): void {
    const record = asRecord(payload);
    if (!record) {
        return;
    }

    const channelId = record['channel_id'];
    const revision = record['revision'];

    if (typeof channelId !== 'string' || typeof revision !== 'number') {
        return;
    }

    lastSeenRevisionByChannel.set(channelId, revision);
}

function handleSharedStateInboundPayload(serializedInbound: string): void {
    dispatchSharedStateEvent('shared-state-message', serializedInbound);

    let payload: unknown;
    try {
        payload = JSON.parse(serializedInbound);
    } catch (error) {
        console.error(
            'Failed to parse shared-state transport inbound payload',
            error,
        );
        return;
    }

    const variant = extractInboundVariant(payload);
    if (!variant) {
        return;
    }

    const [type, value] = variant;
    const valueJson = toSerializedPayload(value);

    switch (type) {
        case 'Snapshot':
            updateLastSeenRevision(value);
            dispatchSharedStateEvent('shared-state-snapshot', valueJson);
            break;
        case 'Event':
            updateLastSeenRevision(value);
            dispatchSharedStateEvent('shared-state-event', valueJson);
            break;
        case 'CommandAccepted':
            dispatchSharedStateEvent(
                'shared-state-command-accepted',
                valueJson,
            );
            break;
        case 'CommandRejected':
            dispatchSharedStateEvent(
                'shared-state-command-rejected',
                valueJson,
            );
            break;
        case 'Pong':
            dispatchSharedStateEvent('shared-state-pong', valueJson);
            break;
        default:
            break;
    }
}

function sharedStateTransportPath(): string {
    if (!sharedStateSessionId) {
        return SHARED_STATE_TRANSPORT_POST_PATH;
    }

    const url = new URL(
        SHARED_STATE_TRANSPORT_POST_PATH,
        window.location.origin,
    );
    url.searchParams.set('session_id', sharedStateSessionId);
    return `${url.pathname}${url.search}`;
}

async function postSharedStateTransport(
    outbound: SharedStateTransportOutbound,
): Promise<boolean> {
    if (!sharedStateSessionId) {
        return false;
    }

    try {
        const response = await fetch(sharedStateTransportPath(), {
            method: 'POST',
            headers: {
                'content-type': 'application/json',
            },
            body: JSON.stringify(outbound),
        });

        if (response.status >= 400) {
            console.error('Shared-state transport post failed', {
                status: response.status,
                statusText: response.statusText,
            });
            return false;
        }

        return true;
    } catch (error) {
        console.error('Shared-state transport post errored', error);
        return false;
    }
}

async function subscribeChannel(channelId: string): Promise<boolean> {
    return postSharedStateTransport({
        Subscribe: {
            channel_id: channelId,
            last_seen_revision:
                lastSeenRevisionByChannel.get(channelId) ?? null,
        },
    });
}

async function unsubscribeChannel(channelId: string): Promise<boolean> {
    return postSharedStateTransport({
        Unsubscribe: {
            channel_id: channelId,
        },
    });
}

async function reconcileChannelSubscriptions(): Promise<void> {
    if (!sharedStateConnected) {
        return;
    }

    for (const channelId of desiredChannels) {
        if (!subscribedChannels.has(channelId)) {
            if (await subscribeChannel(channelId)) {
                subscribedChannels.add(channelId);
            }
        }
    }

    for (const channelId of [...subscribedChannels]) {
        if (!desiredChannels.has(channelId)) {
            if (await unsubscribeChannel(channelId)) {
                subscribedChannels.delete(channelId);
            }
        }
    }
}

function setDesiredChannels(channels: Set<string>): void {
    desiredChannels.clear();
    channels.forEach((channel) => desiredChannels.add(channel));
}

function connectSharedStateTransportStream(): void {
    if (!hasActiveEventSourceStream(SHARED_STATE_STREAM_KEY)) {
        sharedStateConnected = false;
    }

    sharedStateSessionId = startEventSourceStream(
        SHARED_STATE_TRANSPORT_SSE_PATH,
        {
            streamKey: SHARED_STATE_STREAM_KEY,
            includeSessionIdQuery: true,
            onopen: async (response) => {
                if (response.status >= 400) {
                    const status = response.status.toString();
                    console.error('Failed to open shared-state SSE stream', {
                        status,
                    });
                    sharedStateConnected = false;
                    return;
                }

                sharedStateConnected = true;
                dispatchSharedStateEvent(
                    'shared-state-connected',
                    JSON.stringify({ session_id: sharedStateSessionId }),
                );
                await reconcileChannelSubscriptions();
            },
            onmessage: (message) =>
                handleSharedStateInboundPayload(message.data),
            onerror: (error) => {
                sharedStateConnected = false;
                if (error && typeof error === 'object' && 'message' in error) {
                    console.error(
                        'Shared-state SSE stream error',
                        error.message,
                    );
                } else {
                    console.error('Shared-state SSE stream error', error);
                }
            },
            streamIdStorageKey: SHARED_STATE_SESSION_STORAGE_KEY,
            streamIdCookieName: SHARED_STATE_SESSION_COOKIE_NAME,
        },
    );
}

async function refreshSharedStateChannels(): Promise<void> {
    const channels = getDesiredChannelsFromDocument();
    setDesiredChannels(channels);

    if (channels.size === 0) {
        await reconcileChannelSubscriptions();
        return;
    }

    connectSharedStateTransportStream();
    await reconcileChannelSubscriptions();
}

export function initSharedStateTransport(): void {
    void refreshSharedStateChannels();

    on('domLoad', () => {
        void refreshSharedStateChannels();
    });
}

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initSharedStateTransport);
} else {
    initSharedStateTransport();
}
