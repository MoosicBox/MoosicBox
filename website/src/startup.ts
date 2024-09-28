import { init, setProperty } from '@free-log/node-client';
import { appState } from '~/services/app';
import { registerPlayer } from '~/services/player';
import {
    $connectionId,
    InboundMessageType,
    connectionName,
    onConnect,
    onMessage,
    wsService,
} from '~/services/ws';
import { createPlayer as createHowlerPlayer } from '~/services/howler-player';
import { startSilence } from './services/silence-player';
import { isServer } from 'solid-js/web';
import { connections } from './services/api';

if (!isServer) {
    if (
        connections.get().length === 0 &&
        !window.location.pathname.startsWith('/setup/')
    ) {
        window.location.href = '/setup/hello';
    }
}

init({
    logWriterApiUrl: 'https://logs.moosicbox.com',
    shimConsole: true,
    logLevel: 'WARN',
});

setProperty('connectionId', $connectionId());
setProperty('connectionName', connectionName.get());

function updatePlayer() {
    appState.connection?.players
        .filter((player) => player.audioOutputId === 'HOWLER')
        .forEach((player) => {
            registerPlayer(createHowlerPlayer(player.playerId));
        });
}

onMessage((data) => {
    if (data.type === InboundMessageType.CONNECTIONS) {
        updatePlayer();
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

onConnect((id) => {
    updateConnection($connectionId() ?? id, connectionName.get());
    setProperty('connectionId', $connectionId());
});
connectionName.listen((connectionName) => {
    updateConnection($connectionId()!, connectionName);
    setProperty('connectionName', connectionName);
});

startSilence();
