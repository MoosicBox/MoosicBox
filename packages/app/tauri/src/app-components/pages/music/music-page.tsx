import './music-page.css';
import { createSignal, For, onMount, Show } from 'solid-js';
import { open } from '@tauri-apps/plugin-dialog';
import { onlyUnique } from '~/services/util';
import {
    api,
    connections,
    getNewConnectionId,
    setConnection,
} from '~/services/api';
import { htmx } from '~/middleware/htmx';
import { config } from '~/config';

export default function musicPage() {
    let root: HTMLDivElement;

    const [folders, setFolders] = createSignal<string[]>([]);
    const [qobuzAuthSuccess, setQobuzAuthSuccess] = createSignal<boolean>();
    const [tidalAuthSuccess, setTidalAuthSuccess] = createSignal<boolean>();

    async function addFolder() {
        const directories = await open({
            multiple: true,
            directory: true,
        });
        if (directories) {
            setFolders([...folders(), ...directories].filter(onlyUnique));
        }
    }

    async function saveFolders() {
        await api.enableScanOrigin('LOCAL');
        await Promise.all(
            folders().map((folder) => {
                return api.addScanPath(folder);
            }),
        );
        await api.startScan(['LOCAL']);
    }

    async function scanQobuz() {
        await api.enableScanOrigin('QOBUZ');
        await api.startScan(['QOBUZ']);
    }

    async function scanTidal() {
        await api.enableScanOrigin('TIDAL');
        await api.startScan(['TIDAL']);
    }

    async function finish() {
        const requests = [];

        if (folders().length > 0) {
            requests.push(saveFolders());
        }

        if (qobuzAuthSuccess() === true) {
            requests.push(scanQobuz());
        }

        if (tidalAuthSuccess() === true) {
            requests.push(scanTidal());
        }

        await Promise.all(requests);

        localStorage.removeItem('settingUp');
    }

    onMount(async () => {
        htmx.process(root);

        if (config.bundled && connections.get().length === 0) {
            await setConnection(getNewConnectionId(), {
                name: 'Bundled',
                apiUrl: 'http://localhost:8016',
            });
        } else {
            document.body.dispatchEvent(new Event('load-new-profile'));
        }

        root.addEventListener('qobuz-login-attempt', (e) => {
            if (!('detail' in e))
                throw new Error(`Invalid qobuz-login-attempt event`);

            type QobuzLoginAttempt = {
                success: boolean;
            };

            const attempt = e.detail as QobuzLoginAttempt;

            setQobuzAuthSuccess(attempt.success);
        });
        root.addEventListener('tidal-login-attempt', (e) => {
            if (!('detail' in e))
                throw new Error(`Invalid tidal-login-attempt event`);

            type TidalLoginAttempt = {
                success: boolean;
            };

            const attempt = e.detail as TidalLoginAttempt;

            setTidalAuthSuccess(attempt.success);
        });
    });

    return (
        <div ref={root!}>
            <section class="setup-music-page-local-music">
                <h1>Local Music</h1>
                <p>Where do you store your music?</p>
                <button
                    onClick={addFolder}
                    type="button"
                    class="remove-button-styles moosicbox-button"
                >
                    Add Folder
                </button>
                <Show when={folders()}>
                    {(folders) => (
                        <For each={folders()}>
                            {(folder) => <p>{folder}</p>}
                        </For>
                    )}
                </Show>
                <button
                    onClick={saveFolders}
                    type="button"
                    class="remove-button-styles moosicbox-button"
                >
                    Save
                </button>
            </section>
            <hr />
            <section class="setup-music-page-tidal-music">
                <h1>Tidal</h1>
                <p>Sign in to your Tidal account (optional)</p>
                <div
                    hx-get={`/admin/tidal/settings`}
                    hx-trigger="connection-updated from:body, load-new-profile from:body"
                >
                    loading...
                </div>
            </section>
            <hr />
            <section class="setup-music-page-qobuz-music">
                <h1>Qobuz</h1>
                <p>Sign in to your Qobuz account (optional)</p>
                <div
                    hx-get={`/admin/qobuz/settings`}
                    hx-trigger="connection-updated from:body, load-new-profile from:body"
                >
                    loading...
                </div>
            </section>
            <button
                onClick={async () => {
                    await finish();
                    window.location.href = '/';
                }}
                type="button"
                class="remove-button-styles moosicbox-button"
            >
                Finish
            </button>
        </div>
    );
}
