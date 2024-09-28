import './ScanSettings.css';
import { createSignal, Show, For, onMount } from 'solid-js';
import { open } from '@tauri-apps/plugin-dialog';
import { config } from '~/config';
import { api } from '~/services/api';

export default function scanSettingsRender() {
    return config.bundled
        ? bundledScanSettingsRender()
        : clientScanSettingsRender();
}

export function bundledScanSettingsRender() {
    const [folders, setFolders] = createSignal<string[]>([]);

    async function addFolder() {
        const directories = await open({
            multiple: true,
            directory: true,
        });
        if (directories) {
            await saveFolders(directories);
        }
    }

    async function saveFolders(directories: string[]) {
        await api.enableScanOrigin('LOCAL');
        await Promise.all(
            directories.map((folder) => {
                return api.addScanPath(folder);
            }),
        );
        await Promise.all([
            api.startScan(['LOCAL']),
            (async () => {
                const { paths } = await api.getScanPaths();
                setFolders(paths);
            })(),
        ]);
    }

    onMount(async () => {
        const { paths } = await api.getScanPaths();
        setFolders(paths);
    });

    return (
        <div class="scan-settings-container">
            <p>Add new music storage locations:</p>
            <div>
                <Show when={folders()}>
                    {(folders) => (
                        <For each={folders()}>
                            {(folder) => <p>{folder}</p>}
                        </For>
                    )}
                </Show>
                <button
                    onClick={addFolder}
                    type="button"
                    class="remove-button-styles moosicbox-button"
                >
                    Add Folder
                </button>
            </div>
            <button
                onClick={async () => api.startScan(['LOCAL'])}
                type="button"
                class="remove-button-styles moosicbox-button"
            >
                Scan
            </button>
        </div>
    );
}

export function clientScanSettingsRender() {
    return (
        <div class="scan-settings-container">
            <button
                onClick={async () => api.startScan(['LOCAL'])}
                type="button"
                class="remove-button-styles moosicbox-button"
            >
                Scan
            </button>
        </div>
    );
}
