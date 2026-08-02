export type SharedStateLifecycleEvent =
    | 'shared-state-connecting'
    | 'shared-state-connected'
    | 'shared-state-reconnecting'
    | 'shared-state-disconnected';

export function streamStartLifecycle(wasConnected: boolean): SharedStateLifecycleEvent {
    return wasConnected ? 'shared-state-reconnecting' : 'shared-state-connecting';
}

export function streamErrorLifecycle(wasConnected: boolean): SharedStateLifecycleEvent {
    return wasConnected ? 'shared-state-disconnected' : 'shared-state-reconnecting';
}
