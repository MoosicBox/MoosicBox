import './audio-zones.css';
import { produce } from 'solid-js/store';
import { Index, createComputed, createSignal } from 'solid-js';
import { api, Api } from '~/services/api';
import {
    currentPlaybackTarget,
    playerState,
    setCurrentPlaybackTarget,
    setPlayerState,
} from '~/services/player';
import { appState } from '~/services/app';
import Modal from '../Modal';

type PlayerWithConnection = Api.Player & {
    connection: Api.Connection | undefined;
};
type AudioZoneWithConnections = Omit<Api.AudioZone, 'players'> & {
    players: PlayerWithConnection[];
};

export default function audioZonesFunc() {
    let audioZoneNameRef: HTMLInputElement | undefined;

    const [connectionPlayers, setConnectionPlayers] = createSignal<
        PlayerWithConnection[]
    >([]);
    const [audioZones, setAudioZones] = createSignal<
        AudioZoneWithConnections[]
    >([]);
    const [activeAudioZone, setActiveAudioZone] = createSignal(
        playerState.currentAudioZone,
    );
    const [editingAudioZoneName, setEditingAudioZoneName] =
        createSignal<Api.UpdateAudioZone>();
    const [activePlayersZone, setActivePlayersZone] =
        createSignal<AudioZoneWithConnections>();

    const [connections, setConnections] = createSignal<Api.Connection[]>([]);

    function getPlayersWithConnections(): PlayerWithConnection[] {
        const current = appState.connection;

        return (
            current?.players?.map((player) => {
                return {
                    ...player,
                    connection: current,
                };
            }) ?? []
        );
    }

    function getAudioZonesWithConnections(): AudioZoneWithConnections[] {
        const connections = appState.connections;

        return playerState.audioZones.map((zone) => {
            const zoneWithConnections: AudioZoneWithConnections = {
                ...zone,
                players: zone.players.map((player) => {
                    return {
                        ...player,
                        connection: connections.find((c) => {
                            return c.players?.find(
                                (p) => p.playerId === player.playerId,
                            );
                        }),
                    };
                }),
            };
            return zoneWithConnections;
        });
    }

    createComputed(() => {
        setConnectionPlayers(getPlayersWithConnections());
        setAudioZones(getAudioZonesWithConnections());
        setActiveAudioZone(playerState.currentAudioZone);

        const alive = appState.connections.filter((c) => c.alive);
        const dead = appState.connections.filter((c) => !c.alive);

        const aliveCurrent = alive.filter(
            (a) => a.connectionId === appState.connection?.connectionId,
        );
        const aliveOthers = alive.filter(
            (a) => a.connectionId !== appState.connection?.connectionId,
        );

        setConnections([...aliveCurrent, ...aliveOthers, ...dead]);
    });

    function activateConnectionPlayer(player: PlayerWithConnection) {
        setPlayerState(
            produce((state) => {
                state.currentAudioZone = undefined;
                const connectionId = player.connection?.connectionId;
                if (connectionId) {
                    setCurrentPlaybackTarget({
                        type: 'CONNECTION_OUTPUT',
                        connectionId,
                        outputId: player.audioOutputId,
                    });
                }
            }),
        );
    }

    function activateAudioZone(zone: Api.AudioZone) {
        setPlayerState(
            produce((state) => {
                state.currentAudioZone = zone;
                setCurrentPlaybackTarget({
                    type: 'AUDIO_ZONE',
                    audioZoneId: zone.id,
                });
            }),
        );
    }

    async function updateAudioZone(
        update: Api.UpdateAudioZone,
    ): Promise<Api.AudioZone> {
        setPlayerState(
            produce((state) => {
                if (state.currentAudioZone?.id === update.id) {
                    Object.assign(state.currentAudioZone, update);
                }

                const zone = state.audioZones.find((z) => z.id === update.id);

                if (zone) {
                    Object.assign(zone, { name: update.name ?? zone.name });
                }
            }),
        );

        return await api.updateAudioZone(update);
    }

    async function deleteAudioZone(zone: Api.AudioZone) {
        let index: number | undefined;

        setPlayerState(
            produce((state) => {
                if (zone.id === state.currentAudioZone?.id) {
                    const newZone = state.audioZones[0];
                    state.currentAudioZone = newZone;
                    if (newZone) {
                        setCurrentPlaybackTarget({
                            type: 'AUDIO_ZONE',
                            audioZoneId: newZone.id,
                        });
                    } else {
                        setCurrentPlaybackTarget(undefined);
                    }
                }

                index = state.audioZones.findIndex((x) => x.id === zone.id);
                state.audioZones.splice(index, 1);
            }),
        );

        await api.deleteAudioZone(zone.id);
    }

    function replaceZone(
        existing: AudioZoneWithConnections,
        zone: Partial<Api.AudioZone>,
    ) {
        setPlayerState(
            produce((state) => {
                if (state.currentAudioZone?.id === existing.id) {
                    Object.assign(state.currentAudioZone, zone);
                }

                const stateZone = state.audioZones.find(
                    (z) => z.id === existing.id,
                );

                if (stateZone) {
                    Object.assign(stateZone, zone);
                }
            }),
        );
    }

    async function disableAudioPlayer(
        zone: AudioZoneWithConnections,
        player: Api.Player,
    ) {
        const players = zone.players
            .filter((p) => p.playerId !== player.playerId)
            .map(({ playerId }) => playerId);
        const update: Api.UpdateAudioZone = { id: zone.id, players };

        const newZone = await updateAudioZone(update);
        replaceZone(zone, newZone);

        const zones = getAudioZonesWithConnections();
        setActivePlayersZone(zones.find((x) => x.id === newZone.id));
    }

    async function enableAudioPlayer(
        zone: AudioZoneWithConnections,
        player: Api.Player,
    ) {
        const players = [
            ...zone.players.filter((p) => p.playerId !== player.playerId),
            player,
        ].map(({ playerId }) => playerId);
        const update: Api.UpdateAudioZone = { id: zone.id, players };

        const newZone = await updateAudioZone(update);
        replaceZone(zone, newZone);

        const zones = getAudioZonesWithConnections();
        setActivePlayersZone(zones.find((x) => x.id === newZone.id));
    }

    return (
        <div class="audio-zones">
            <div class="audio-zones-list-modal-content">
                <Index each={connectionPlayers()}>
                    {(connectionPlayer) => (
                        <div
                            class={`audio-zones-list-zone${
                                currentPlaybackTarget()?.type ===
                                    'CONNECTION_OUTPUT' &&
                                connectionPlayer().audioOutputId ===
                                    (
                                        currentPlaybackTarget() as Api.ConnectionOutputPlaybackTarget
                                    ).outputId
                                    ? ' active'
                                    : ''
                            }`}
                        >
                            <h2
                                onClick={() =>
                                    activateConnectionPlayer(connectionPlayer())
                                }
                                class="audio-zones-list-zone-header"
                            >
                                {connectionPlayer().name}
                            </h2>
                            <div
                                class={`audio-zone-audio-zone-modal-audio-zone-player`}
                            >
                                <img
                                    class="audio-icon"
                                    src="/img/audio-white.svg"
                                    alt="Zone Player"
                                />{' '}
                                {connectionPlayer().connection?.name} -{' '}
                                {connectionPlayer().name}
                            </div>
                        </div>
                    )}
                </Index>
                <Index each={audioZones()}>
                    {(audioZone) => (
                        <div
                            class={`audio-zones-list-zone${
                                audioZone().id === activeAudioZone()?.id
                                    ? ' active'
                                    : ''
                            }`}
                        >
                            <h2
                                onClick={() => activateAudioZone(audioZone())}
                                class="audio-zones-list-zone-header"
                            >
                                {editingAudioZoneName()?.id ===
                                audioZone().id ? (
                                    <>
                                        <input
                                            ref={audioZoneNameRef!}
                                            autofocus
                                            onClick={(e) =>
                                                e.stopImmediatePropagation()
                                            }
                                            type="text"
                                            value={
                                                editingAudioZoneName()!.name ??
                                                ''
                                            }
                                            onKeyUp={async (e) => {
                                                if (e.key !== 'Enter') return;

                                                e.stopImmediatePropagation();
                                                editingAudioZoneName()!.name =
                                                    audioZoneNameRef!.value;
                                                await updateAudioZone(
                                                    editingAudioZoneName()!,
                                                );
                                                setEditingAudioZoneName(
                                                    undefined,
                                                );
                                            }}
                                        />
                                        <button
                                            class="remove-button-styles audio-zones-list-zone-header-submit-edit-name"
                                            onClick={async (e) => {
                                                e.stopImmediatePropagation();
                                                editingAudioZoneName()!.name =
                                                    audioZoneNameRef!.value;
                                                await updateAudioZone(
                                                    editingAudioZoneName()!,
                                                );
                                                setEditingAudioZoneName(
                                                    undefined,
                                                );
                                            }}
                                        >
                                            <img
                                                class="audio-zones-list-zone-header-submit-edit-name-icon"
                                                src="/img/checkmark-white.svg"
                                                alt="Submit audio zone name"
                                            />
                                        </button>
                                    </>
                                ) : (
                                    <>
                                        {audioZone().name}
                                        <button
                                            class="remove-button-styles audio-zones-list-zone-header-edit-name"
                                            onClick={(e) => {
                                                e.stopImmediatePropagation();
                                                setEditingAudioZoneName({
                                                    id: audioZone().id,
                                                    name: audioZone().name,
                                                });
                                            }}
                                        >
                                            <img
                                                class="audio-zones-list-zone-header-edit-name-icon"
                                                src="/img/pencil-white.svg"
                                                alt="Edit audio zone name"
                                            />
                                        </button>
                                    </>
                                )}
                                <div class="audio-zones-list-zone-header-right">
                                    <div
                                        class="audio-zones-list-zone-header-delete-zone"
                                        onClick={async (e) => {
                                            e.stopImmediatePropagation();
                                            await deleteAudioZone(audioZone());
                                        }}
                                    >
                                        <img
                                            class="trash-icon"
                                            src="/img/trash-white.svg"
                                            alt="Delete playback session"
                                        />
                                    </div>
                                </div>
                            </h2>
                            <Index each={audioZone().players}>
                                {(player) => (
                                    <div
                                        class={`audio-zone-audio-zone-modal-audio-zone-player`}
                                    >
                                        <img
                                            class="audio-icon"
                                            src="/img/audio-white.svg"
                                            alt="Zone Player"
                                        />{' '}
                                        {player().connection?.name} -{' '}
                                        {player().name}
                                    </div>
                                )}
                            </Index>
                            <button
                                class={`remove-button-styles audio-zone-audio-zone-modal-audio-zone-player add-players`}
                                onClick={(e) => {
                                    e.stopImmediatePropagation();
                                    setActivePlayersZone(audioZone());
                                }}
                            >
                                <img
                                    class="plus-icon"
                                    src="/img/plus-white.svg"
                                    alt="Add players to audio zone"
                                />{' '}
                                Add players
                            </button>
                        </div>
                    )}
                </Index>
                <Modal
                    show={() => activePlayersZone()}
                    onClose={() => setActivePlayersZone(undefined)}
                    class="audio-zone-active-players-modal"
                >
                    {(activePlayersZone) => (
                        <div class="audio-zone-active-players-modal-container">
                            <div class="audio-zone-active-players-modal-header">
                                <h1>
                                    {activePlayersZone.name} - Active Players
                                </h1>
                                <div
                                    class="audio-zone-active-players-modal-header-close"
                                    onClick={(e) => {
                                        setActivePlayersZone(undefined);
                                        e.stopImmediatePropagation();
                                    }}
                                >
                                    <img
                                        class="cross-icon"
                                        src="/img/cross-white.svg"
                                        alt="Close playlist sessions modal"
                                    />
                                </div>
                            </div>
                            <div class="audio-zone-active-players-modal-content">
                                <Index each={connections()}>
                                    {(connection) => (
                                        <div
                                            class={`audio-zone-active-players-modal-connection${
                                                connection().alive
                                                    ? ' alive'
                                                    : ' dead'
                                            }`}
                                        >
                                            <Index each={connection().players}>
                                                {(player) => (
                                                    <div
                                                        class={`audio-zone-active-players-modal-connection-player${
                                                            activePlayersZone.players.some(
                                                                (p) =>
                                                                    p.playerId ===
                                                                    player()
                                                                        .playerId,
                                                            )
                                                                ? ' active'
                                                                : ''
                                                        }`}
                                                    >
                                                        {connection().name} -{' '}
                                                        {player().name}{' '}
                                                        {activePlayersZone.players.some(
                                                            (p) =>
                                                                p.playerId ===
                                                                player()
                                                                    .playerId,
                                                        ) ? (
                                                            <img
                                                                class="audio-icon"
                                                                src="/img/audio-white.svg"
                                                                alt="Player enabled"
                                                                onClick={() =>
                                                                    disableAudioPlayer(
                                                                        activePlayersZone,
                                                                        player(),
                                                                    )
                                                                }
                                                            />
                                                        ) : (
                                                            <img
                                                                class="audio-icon"
                                                                src="/img/audio-off-white.svg"
                                                                alt="Player disabled"
                                                                onClick={() =>
                                                                    enableAudioPlayer(
                                                                        activePlayersZone,
                                                                        player(),
                                                                    )
                                                                }
                                                            />
                                                        )}
                                                    </div>
                                                )}
                                            </Index>
                                        </div>
                                    )}
                                </Index>
                            </div>
                        </div>
                    )}
                </Modal>
            </div>
        </div>
    );
}
