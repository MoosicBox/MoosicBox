import './playback-sessions-modal.css';
import Modal from '../Modal';
import PlaybackSessions from '../PlaybackSessions';
import { currentPlaybackTarget } from '~/services/player';
import { showPlaybackSessions, triggerStartup } from '~/services/app';
import { clientSignal } from '~/services/util';
import { onMount } from 'solid-js';
import { wsService } from '~/services/ws';

export default function playbackSessionsModalFunc() {
    onMount(async () => {
        await triggerStartup();
    });
    const [$showPlaybackSessions] = clientSignal(showPlaybackSessions);

    function createNewSession() {
        wsService.createSession({
            name: 'New Session',
            playlist: {
                tracks: [],
            },
            playbackTarget: currentPlaybackTarget(),
        });
    }

    return (
        <div data-turbo-permanent id="playback-sessions-modal">
            <Modal
                show={() => $showPlaybackSessions()}
                onClose={() => showPlaybackSessions.set(false)}
            >
                <div class="playback-sessions-modal-container">
                    <div class="playback-sessions-modal-header">
                        <h1>Playback Sessions</h1>
                        <button
                            class="playback-sessions-modal-header-new-button"
                            onClick={() => createNewSession()}
                        >
                            New
                        </button>
                        <div
                            class="playback-sessions-modal-close"
                            onClick={(e) => {
                                showPlaybackSessions.set(false);
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
                    <div class="playback-sessions-modal-content">
                        <PlaybackSessions />
                    </div>
                </div>
            </Modal>
        </div>
    );
}
