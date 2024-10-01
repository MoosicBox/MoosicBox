import './playback-quality-modal.css';
import Modal from '../Modal';
import PlaybackQuality from '../PlaybackQuality';
import { showPlaybackQuality } from '~/services/app';
import { clientSignal } from '~/services/util';

export default function playbackQualityModalFunc() {
    const [$showPlaybackQuality] = clientSignal(showPlaybackQuality);

    return (
        <div data-turbo-permanent id="playback-quality-modal">
            <Modal
                show={() => $showPlaybackQuality()}
                onClose={() => showPlaybackQuality.set(false)}
            >
                <div class="playback-quality-modal-container">
                    <div class="playback-quality-modal-header">
                        <h1>Playback Quality</h1>
                        <div
                            class="playback-quality-modal-close"
                            onClick={(e) => {
                                showPlaybackQuality.set(false);
                                e.stopImmediatePropagation();
                            }}
                        >
                            <img
                                class="cross-icon"
                                src="/img/cross-white.svg"
                                alt="Close playlist quality modal"
                            />
                        </div>
                    </div>
                    <div class="playback-quality-modal-content">
                        <PlaybackQuality />
                    </div>
                </div>
            </Modal>
        </div>
    );
}
