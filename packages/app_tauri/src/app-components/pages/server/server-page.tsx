import './server-page.css';
import { createSignal, For, onCleanup, onMount } from 'solid-js';
import { invoke } from '@tauri-apps/api/core';
import {
    api,
    connections,
    getNewConnectionId,
    setConnection,
} from '~/services/api';

type Server = {
    id: string;
    name: string;
    host: string;
    dns: string;
};

export default function serverPage() {
    let serverAddressInput: HTMLInputElement;

    const [intervalHandle, setIntervalHandle] = createSignal<NodeJS.Timeout>();
    const [servers, setServers] = createSignal<Server[]>([]);

    onMount(async () => {
        setIntervalHandle(
            setInterval(async () => {
                const servers = await invoke<Server[]>(
                    'fetch_moosicbox_servers',
                );
                setServers(servers);
            }, 1000),
        );
    });

    onCleanup(async () => {
        const handle = intervalHandle();

        if (handle) {
            clearInterval(handle);
        }
    });

    async function selectServer(server: Server) {
        const existing = connections
            .get()
            .find((x) => x.apiUrl === server.host);

        if (existing) {
            await setConnection(existing.id, existing);
        } else {
            await setConnection(getNewConnectionId(), {
                name: server.name,
                apiUrl: server.host,
            });
        }

        window.location.href = './profile';
    }

    async function saveManualServerAddress() {
        const existing = connections
            .get()
            .find((x) => x.apiUrl === serverAddressInput.value);

        if (existing) {
            await setConnection(existing.id, existing);
        } else {
            await setConnection(getNewConnectionId(), {
                name: 'MoosicBox Server',
                apiUrl: serverAddressInput.value,
            });
        }

        try {
            await api.getAlbums({ limit: 0 });
        } catch (e) {
            console.error('Invalid server:', e);
            throw e;
        }

        window.location.href = './profile';
    }

    return (
        <div>
            <h1>Set up your MoosicBox server connection:</h1>
            <hr />
            {servers().length === 0 && (
                <h2>Searching for compatible MoosicBox servers...</h2>
            )}
            {servers().length > 0 && <h2>Select your MoosicBox server</h2>}
            <For each={servers()}>
                {(server) => (
                    <div class="server-page-server">
                        <div>
                            {server.name} - {server.host}
                        </div>
                        <div>
                            <button
                                onClick={async () => await selectServer(server)}
                                type="button"
                                class="remove-button-styles select-button"
                            >
                                Select
                            </button>
                        </div>
                    </div>
                )}
            </For>
            <h2>or specify an address manually:</h2>
            <input
                ref={serverAddressInput!}
                type="text"
                placeholder="http://localhost:8000"
            />
            <button
                onClick={async () => await saveManualServerAddress()}
                type="button"
                class="remove-button-styles finish-button"
            >
                Save
            </button>
        </div>
    );
}
