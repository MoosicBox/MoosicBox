import './DownloadSettings.css';
import { createSignal, Show, For, onMount } from 'solid-js';
import { open } from '@tauri-apps/plugin-dialog';
import { Api, api, defaultDownloadLocation } from '~/services/api';
import { clientSignal } from '~/services/util';

export default function downloadSettingsRender() {
    const [locations, setLocations] = createSignal<Api.DownloadLocation[]>([]);
    const [$defaultDownloadLocation] = clientSignal(defaultDownloadLocation);

    async function addLocation() {
        const directories = await open({
            multiple: true,
            directory: true,
        });
        if (directories) {
            await saveLocations(directories);
        }
    }

    async function saveLocations(directories: string[]) {
        await Promise.all(
            directories.map((location) => {
                return api.addDownloadLocation(location);
            }),
        );
        const { items } = await api.getDownloadLocations();
        setLocations(items);
    }

    async function setAsDefault(location: Api.DownloadLocation) {
        defaultDownloadLocation.set(location.id);
    }

    onMount(async () => {
        const { items } = await api.getDownloadLocations();
        setLocations(items);
    });

    return (
        <div class="download-settings-container">
            <p>Download locations:</p>
            <div>
                <Show when={locations()}>
                    {(locations) => (
                        <For each={locations()}>
                            {(location) => (
                                <p>
                                    {location.id ===
                                        $defaultDownloadLocation() && (
                                        <span>*</span>
                                    )}{' '}
                                    {location.path}
                                    <button
                                        onClick={async () =>
                                            await setAsDefault(location)
                                        }
                                        type="button"
                                        class="remove-button-styles moosicbox-button"
                                    >
                                        Set as default
                                    </button>
                                </p>
                            )}
                        </For>
                    )}
                </Show>
                <button
                    onClick={addLocation}
                    type="button"
                    class="remove-button-styles moosicbox-button"
                >
                    Add location
                </button>
            </div>
        </div>
    );
}
