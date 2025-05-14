import './error-modal.css';
import Modal from '../Modal';
import { clientSignal } from '~/services/util';
import { clearErrorMessages, errorMessages } from '~/services/app';
import { For } from 'solid-js';

export default function changePlaybackTargetModalFunc() {
    const [$errorMessages] = clientSignal(errorMessages);

    return (
        <div data-turbo-permanent id="error-modal">
            <Modal
                show={() => $errorMessages().length > 0}
                onClose={() => clearErrorMessages()}
            >
                <div class="error-modal-container">
                    <div class="error-modal-header">
                        <h1>Error</h1>
                        <div
                            class="error-modal-close"
                            onClick={(e) => {
                                clearErrorMessages();
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
                    <div class="error-modal-content">
                        <div class="error-modal-content-error-messages">
                            <For each={$errorMessages()}>
                                {(x) => (
                                    <div class="error-modal-content-error-message">
                                        {x}
                                    </div>
                                )}
                            </For>
                        </div>
                    </div>
                </div>
            </Modal>
        </div>
    );
}
