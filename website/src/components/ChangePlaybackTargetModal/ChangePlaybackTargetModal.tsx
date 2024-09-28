import './change-playback-target-modal.css';
import Modal from '../Modal';
import { showChangePlaybackTargetModal } from '~/services/app';
import { clientSignal } from '~/services/util';

let responsePromiseResolves: ((yes: boolean) => void)[] = [];

export async function responsePromise(): Promise<boolean> {
    return new Promise((resolve) => {
        responsePromiseResolves.push(resolve);
    });
}

export default function changePlaybackTargetModalFunc() {
    const [$showChangePlaybackTargetModal] = clientSignal(
        showChangePlaybackTargetModal,
    );

    return (
        <div data-turbo-permanent id="change-playback-target-modal">
            <Modal
                show={() => $showChangePlaybackTargetModal()}
                onClose={() => showChangePlaybackTargetModal.set(false)}
            >
                <div class="change-playback-target-modal-container">
                    <div class="change-playback-target-modal-header">
                        <h1>Confirm</h1>
                        <div
                            class="change-playback-target-modal-close"
                            onClick={(e) => {
                                showChangePlaybackTargetModal.set(false);
                                e.stopImmediatePropagation();
                            }}
                        >
                            <img
                                class="cross-icon"
                                src="/img/cross-white.svg"
                                alt="Close change playback target modal"
                            />
                        </div>
                    </div>
                    <div class="change-playback-target-modal-content">
                        Change playback target?
                        <button
                            class="remove-button-styles change-playback-target-modal-confirmation-button"
                            type="button"
                            onClick={(e) => {
                                responsePromiseResolves.forEach((x) => x(true));
                                responsePromiseResolves = [];
                                showChangePlaybackTargetModal.set(false);
                                e.stopImmediatePropagation();
                            }}
                        >
                            yes
                        </button>
                        <button
                            class="remove-button-styles change-playback-target-modal-confirmation-button"
                            type="button"
                            onClick={(e) => {
                                responsePromiseResolves.forEach((x) =>
                                    x(false),
                                );
                                responsePromiseResolves = [];
                                showChangePlaybackTargetModal.set(false);
                                e.stopImmediatePropagation();
                            }}
                        >
                            no
                        </button>
                    </div>
                </div>
            </Modal>
        </div>
    );
}
