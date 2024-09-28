import './profile-page.css';
import { createEffect, createSignal, on, onMount, Show } from 'solid-js';
import {
    connection,
    connections,
    getNewConnectionId,
    refreshConnectionProfiles,
    setConnection,
} from '~/services/api';
import { htmx } from '~/middleware/htmx';
import { config } from '~/config';
import { clientSignal } from '~/services/util';

export default function profilePage() {
    let root: HTMLDivElement;

    const [$connection] = clientSignal(connection);
    const [errorMessage, setErrorMessage] = createSignal<string>();
    const [showProfiles, setShowProfiles] = createSignal(
        ($connection()?.profiles?.length ?? 0) > 0,
    );

    createEffect(
        on($connection, (con) => {
            if (con?.profiles) {
                const newValue = con.profiles.length > 0;

                if (newValue !== showProfiles()) {
                    setShowProfiles(newValue);
                    setTimeout(() => {
                        htmx.process(root);
                    }, 0);
                }
            }
        }),
    );

    onMount(async () => {
        htmx.process(root);

        if (connections.get().length === 0) {
            await setConnection(getNewConnectionId(), {
                name: 'Bundled',
                apiUrl: 'http://localhost:8016',
            });
        } else {
            document.body.dispatchEvent(new Event('load-new-profile'));
        }

        root.addEventListener('create-moosicbox-profile', async (e) => {
            if (!('detail' in e))
                throw new Error(`Invalid create-moosicbox-profile event`);

            setErrorMessage(undefined);

            type CreateMoosicBoxProfile = {
                success: boolean;
                message: string;
                profile?: string | undefined;
            };

            const attempt = e.detail as CreateMoosicBoxProfile;

            if (!attempt.success) {
                setErrorMessage(attempt.message);
                return;
            }

            if (attempt.profile) {
                const con = connection.get();

                if (con) {
                    const updated = await setConnection(con.id, {
                        profile: attempt.profile,
                    });
                    await refreshConnectionProfiles(updated);

                    nextStep();
                }
            }
        });

        root.addEventListener('select-moosicbox-profile', async (e) => {
            if (!('detail' in e))
                throw new Error(`Invalid create-moosicbox-profile event`);

            setErrorMessage(undefined);

            type SelectMoosicBoxProfile = {
                profile: string;
            };

            const attempt = e.detail as SelectMoosicBoxProfile;

            const con = connection.get();

            if (con) {
                const updated = await setConnection(con.id, {
                    profile: attempt.profile,
                });
                await refreshConnectionProfiles(updated);
            }
        });
    });

    function nextStep() {
        if (config.bundled) {
            window.location.href = './music';
        } else {
            localStorage.removeItem('settingUp');
            window.location.href = '/';
        }
    }

    return (
        <div ref={root!}>
            <section class="setup-profile-page-local-profile">
                <h1>Setup your profile</h1>
                <Show when={showProfiles()}>
                    <>
                        <hr />
                        <h2>Select from existing profiles:</h2>
                        <div
                            hx-get={`/admin/profiles/select`}
                            hx-trigger="load"
                            hx-swap="outerHTML"
                        >
                            loading...
                        </div>
                        <button
                            onClick={nextStep}
                            type="button"
                            class="remove-button-styles moosicbox-button"
                        >
                            {config.bundled ? 'Next' : 'Finish'}
                        </button>
                        <h2>Or create a new one:</h2>
                    </>
                </Show>
                <div
                    hx-get={`/admin/profiles/new`}
                    hx-trigger="connection-changed from:body, load-new-profile from:body"
                >
                    loading...
                </div>
                <Show when={errorMessage()}>
                    {(errorMessage) => <p>{errorMessage()}</p>}
                </Show>
            </section>
        </div>
    );
}
