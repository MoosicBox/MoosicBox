// @refresh reload
import { init, setProperty } from '@free-log/node-client';
import { invoke, InvokeArgs } from '@tauri-apps/api/core';
import { appState, onStartupFirst } from '~/services/app';
import {
    Api,
    ApiType,
    Connection,
    api,
    connection,
    connections,
} from '~/services/api';
import { createPlayer as createHowlerPlayer } from '~/services/howler-player';
import {
    currentPlaybackSessionId,
    currentPlaybackTarget,
    onCurrentPlaybackSessionChanged,
    onCurrentPlaybackTargetChanged,
    registerPlayer,
} from '~/services/player';
import {
    $connectionId,
    InboundMessageType,
    connectionName,
    onConnect,
    onMessage,
    setConnectionId,
    wsService,
} from '~/services/ws';
import { override } from './ws';
import { isServer } from 'solid-js/web';

if (!isServer) {
    const settingUp =
        connections.get().length === 0 || localStorage.getItem('settingUp');
    const setupPath = window.location.pathname.startsWith('/setup/');

    if (settingUp && !setupPath) {
        window.location.href = '/setup/hello';
    }
}

init({
    logWriterApiUrl: 'https://logs.moosicbox.com',
    shimConsole: true,
    logLevel: 'WARN',
});

override();

function tryInvoke(event: string, payload?: InvokeArgs) {
    (async () => {
        try {
            invoke(event, payload);
        } catch (e) {
            console.error(`Failed to invoke '${event}':`, e);
        }
    })();
}

async function updatePlayers() {
    const connection = appState.connections.find(
        (c) => c.connectionId === $connectionId(),
    );

    if (connection?.players) {
        connection.players
            .filter((player) => player.audioOutputId === 'HOWLER')
            .forEach((player) => {
                registerPlayer(createHowlerPlayer(player.playerId));
            });
    }
}

onMessage(async (data) => {
    switch (data.type) {
        case InboundMessageType.CONNECTIONS: {
            await updatePlayers();
            break;
        }
    }
});

function updateConnection(connectionId: string, name: string) {
    wsService.registerConnection({
        connectionId,
        name,
        players: [
            {
                audioOutputId: 'HOWLER',
                name: 'Web Player',
            },
        ],
    });
}

onCurrentPlaybackTargetChanged((playbackTarget) => {
    updateStateForConnection(connection.get(), { playbackTarget });
});

onConnect((id) => {
    setConnectionId(`${connection.get()?.id}`, id);
    updateConnection($connectionId(), connectionName.get());
});
connectionName.listen((name) => {
    updateConnection($connectionId()!, name);
});

const apiOverride: Partial<ApiType> = {};

const originalApi = { ...api };

function updateApi(secure: boolean) {
    if (secure) {
        Object.assign(api, originalApi);
    } else {
        Object.assign(api, apiOverride);
    }
}

type State = {
    connectionId?: string | undefined;
    connectionName?: string | undefined;
    apiUrl?: string | undefined;
    clientId?: string | undefined;
    signatureToken?: string | undefined;
    apiToken?: string | undefined;
    profile?: string | undefined;
    playbackTarget?: Api.PlaybackTarget | undefined;
    currentSessionId?: number | undefined;
};

function updateStateForConnection(con: Connection | null, overrides?: State) {
    if (con?.apiUrl) {
        updateApi(con.apiUrl.toLowerCase().startsWith('https://'));
    }

    const state: State = {
        connectionId: $connectionId(),
        connectionName: con?.name,
        apiUrl: con?.apiUrl,
        clientId: con?.clientId,
        signatureToken: Api.signatureToken(),
        apiToken: con?.token,
        profile: con?.profile,
        playbackTarget: currentPlaybackTarget(),
        currentSessionId: currentPlaybackSessionId(),
    };

    Object.assign(state, overrides);

    console.debug('Setting state', state);

    tryInvoke('set_state', { state });
}

onStartupFirst(async () => {
    tryInvoke('show_main_window');
    tryInvoke('on_startup');

    setProperty('connectionId', $connectionId());
    setProperty('connectionName', connectionName.get());

    updateStateForConnection(connection.get());

    connection.listen(async (con) => {
        updateStateForConnection(con);
    });
    onConnect(async (connectionId) => {
        setProperty('connectionId', connectionId);
        updateStateForConnection(connection.get());
    });
    connectionName.listen(async (connectionName) => {
        setProperty('connectionName', connectionName);
    });
    Api.onSignatureTokenUpdated(async () => {
        updateStateForConnection(connection.get());
    });
    onCurrentPlaybackSessionChanged(() => {
        updateStateForConnection(connection.get());
    });
});
