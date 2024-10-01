import './settings.css';
import { createEffect, createSignal, For, on, onMount, Show } from 'solid-js';
import {
    api,
    connection,
    type Connection,
    setConnection as apiSetConnection,
    deleteConnection as apiDeleteConnection,
    getNewConnectionId,
    connections,
    setActiveConnection,
    setActiveProfile,
} from '~/services/api';
import { clientSignal } from '~/services/util';
import { connectionName } from '~/services/ws';
import { htmx } from '~/middleware/htmx';
import { isServer } from 'solid-js/web';
import ScanSettings from '~/components/ScanSettings';
import { config } from '~/config';
import DownloadSettings from '~/components/DownloadSettings';

export default function settingsPage() {
    let root: HTMLDivElement;

    const [$connections, _setConnections] = clientSignal(connections);
    const [$connection, setConnection] = clientSignal(connection);
    const [$connectionName, setConnectionName] = clientSignal(connectionName);
    const [profiles, setProfiles] = createSignal<string[]>();
    const [profile, setProfile] = createSignal<string>();

    const [status, setStatus] = createSignal<string>();
    const [loading, setLoading] = createSignal(false);

    let clientIdInput: HTMLInputElement;
    let apiUrlInput: HTMLInputElement;
    let nameInput: HTMLInputElement;

    createEffect(
        on($connection, (con) => {
            setProfile(con?.profile);
            setProfiles(con?.profiles);
        }),
    );

    if (!isServer) {
        onMount(() => {
            htmx.process(root);
        });
    }

    async function newConnection() {
        const id = getNewConnectionId();
        setConnection({
            id,
            name: 'New connection',
            apiUrl: '',
            clientId: '',
            token: '',
            staticToken: '',
        });
        await apiSetConnection(id, { name: 'New connection' });
    }

    async function saveConnection(values: Partial<Connection>) {
        const con = $connection();
        const id = con?.id ?? getNewConnectionId();
        setConnection({
            id,
            name: values.name ?? con?.name ?? '',
            apiUrl: values.apiUrl ?? con?.apiUrl ?? '',
            profile: values.profile ?? con?.profile,
            clientId: values.clientId ?? con?.clientId ?? '',
            token: values.token ?? con?.token ?? '',
            staticToken: values.staticToken ?? con?.staticToken ?? '',
        });
        await apiSetConnection(id, values);
    }

    async function deleteConnection(connection: Connection) {
        await apiDeleteConnection(connection);
    }

    async function saveName() {
        await saveConnection({
            name: nameInput.value,
        });
    }

    async function saveApiUrl() {
        const con = $connection();
        await saveConnection({
            apiUrl: apiUrlInput.value,
            staticToken: con?.staticToken ?? '',
        });
    }

    let connectionNameInput: HTMLInputElement;

    function saveConnectionName() {
        setConnectionName(connectionNameInput.value);
    }

    async function saveClientId() {
        await saveConnection({
            clientId: clientIdInput.value,
        });
    }

    let tokenInput: HTMLInputElement;

    async function saveToken() {
        await saveConnection({
            token: tokenInput.value,
        });
    }

    let staticTokenInput: HTMLInputElement;

    async function saveStaticToken() {
        await saveConnection({
            staticToken: staticTokenInput.value,
        });
    }

    let magicTokenInput: HTMLInputElement;

    async function saveMagicToken() {
        const resp = await api.magicToken(magicTokenInput.value);
        setLoading(false);

        if (resp) {
            const con = $connection();
            await saveConnection({
                name: con?.name ?? 'New connection',
                apiUrl: con?.apiUrl ?? '',
                clientId: resp.clientId,
                token: resp.accessToken,
            });
            magicTokenInput.value = '';
            setStatus('Successfully set values');
        } else {
            setStatus('Failed to authenticate with magic token');
        }
    }

    return (
        <div ref={root!}>
            <section>
                <ul>
                    <li>
                        Name:{' '}
                        <input
                            ref={connectionNameInput!}
                            type="text"
                            value={$connectionName()}
                            onKeyUp={(e) =>
                                e.key === 'Enter' && saveConnectionName()
                            }
                        />
                        <button onClick={saveConnectionName}>save</button>
                    </li>
                </ul>

                <Show when={$connections()}>
                    {(connections) => (
                        <select
                            name="connections"
                            id="connections-dropdown"
                            onChange={async (e) => {
                                await setActiveConnection(
                                    parseInt(e.currentTarget.value),
                                );
                            }}
                        >
                            <For each={connections()}>
                                {(con) => (
                                    <option
                                        value={con.id}
                                        selected={con.id === $connection()?.id}
                                    >
                                        {con.name}
                                    </option>
                                )}
                            </For>
                        </select>
                    )}
                </Show>

                <button type="button" onClick={newConnection}>
                    New connection
                </button>

                <Show when={$connection()}>
                    {(connection) => (
                        <button onClick={() => deleteConnection(connection())}>
                            delete
                        </button>
                    )}
                </Show>

                <ul>
                    <li>
                        Name:{' '}
                        <input
                            ref={nameInput!}
                            type="text"
                            value={$connection()?.name ?? 'New connection'}
                            onKeyUp={(e) => e.key === 'Enter' && saveName()}
                        />
                        <button onClick={saveName}>save</button>
                    </li>
                    <select
                        name="connections"
                        id="connections-dropdown"
                        onChange={async (e) => {
                            await setActiveProfile(e.currentTarget.value);
                        }}
                    >
                        <For each={profiles()}>
                            {(p) => (
                                <option value={p} selected={p === profile()}>
                                    {p}
                                </option>
                            )}
                        </For>
                    </select>
                    <li>
                        API Url:{' '}
                        <input
                            ref={apiUrlInput!}
                            type="text"
                            value={$connection()?.apiUrl ?? ''}
                            onKeyUp={(e) => e.key === 'Enter' && saveApiUrl()}
                        />
                        <button onClick={saveApiUrl}>save</button>
                    </li>
                    <li>
                        Client ID:{' '}
                        <input
                            ref={clientIdInput!}
                            type="text"
                            value={$connection()?.clientId ?? ''}
                            onKeyUp={(e) => e.key === 'Enter' && saveClientId()}
                        />
                        <button onClick={saveClientId}>save</button>
                    </li>
                    <li>
                        Token:{' '}
                        <input
                            ref={tokenInput!}
                            type="text"
                            value={$connection()?.token ?? ''}
                            onKeyUp={(e) => e.key === 'Enter' && saveToken()}
                        />
                        <button onClick={saveToken}>save</button>
                    </li>
                    <li>
                        Static Token:{' '}
                        <input
                            ref={staticTokenInput!}
                            type="text"
                            value={$connection()?.staticToken ?? ''}
                            onKeyUp={(e) =>
                                e.key === 'Enter' && saveStaticToken()
                            }
                        />
                        <button onClick={saveStaticToken}>save</button>
                    </li>
                    <li>
                        Magic Token:{' '}
                        <input
                            ref={magicTokenInput!}
                            type="text"
                            onKeyUp={(e) =>
                                e.key === 'Enter' && saveMagicToken()
                            }
                        />
                        <button onClick={saveMagicToken}>save</button>
                    </li>
                </ul>
                {status() && status()}
                {loading() && 'loading...'}
            </section>
            <hr />
            <section>
                <ScanSettings />
            </section>
            {config.bundled && (
                <>
                    <hr />
                    <section>
                        <DownloadSettings />
                    </section>
                </>
            )}
            {$connection() && (
                <>
                    <hr />
                    <section>
                        <h2>Tidal</h2>
                        <div
                            hx-get={`/admin/tidal/settings?showScan=true`}
                            hx-trigger="load, connection-updated from:body"
                        >
                            loading...
                        </div>
                    </section>
                    <hr />
                    <section>
                        <h2>Qobuz</h2>
                        <div
                            hx-get={`/admin/qobuz/settings?showScan=true`}
                            hx-trigger="load, connection-updated from:body"
                        >
                            loading...
                        </div>
                    </section>
                </>
            )}
        </div>
    );
}
