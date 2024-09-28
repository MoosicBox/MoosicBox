import './audio-zones-modal.css';
import Modal from '../Modal';
import AudioZones from '../AudioZones';
import { showAudioZones, triggerStartup } from '~/services/app';
import { clientSignal } from '~/services/util';
import { onMount } from 'solid-js';
import { produce } from 'solid-js/store';
import { api } from '~/services/api';
import { setPlayerState } from '~/services/player';

export default function audioZonesModalFunc() {
    onMount(async () => {
        await triggerStartup();
    });
    const [$showAudioZones] = clientSignal(showAudioZones);

    async function createNewAudioZone() {
        const zone = await api.createAudioZone('Custom Zone');

        setPlayerState(
            produce((state) => {
                state.audioZones.push(zone);
            }),
        );
    }

    return (
        <div data-turbo-permanent id="audio-zones-modal">
            <Modal
                show={() => $showAudioZones()}
                onClose={() => showAudioZones.set(false)}
            >
                <div class="audio-zones-modal-container">
                    <div class="audio-zones-modal-header">
                        <h1>Audio Zones</h1>
                        <button
                            class="playback-sessions-modal-header-new-button"
                            onClick={async () => await createNewAudioZone()}
                        >
                            New
                        </button>
                        <div
                            class="audio-zones-modal-close"
                            onClick={(e) => {
                                showAudioZones.set(false);
                                e.stopImmediatePropagation();
                            }}
                        >
                            <img
                                class="cross-icon"
                                src="/img/cross-white.svg"
                                alt="Close audio zones modal"
                            />
                        </div>
                    </div>
                    <div class="audio-zones-modal-content">
                        <AudioZones />
                    </div>
                </div>
            </Modal>
        </div>
    );
}
