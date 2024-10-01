import './server-page.css';
import { api, getNewConnectionId, setConnection } from '~/services/api';

const id = getNewConnectionId();

export default function serverPage() {
    let serverAddressInput: HTMLInputElement;

    async function saveServerAddress() {
        await setConnection(id, {
            id,
            name: 'MoosicBox Server',
            apiUrl: serverAddressInput.value,
            clientId: '',
            token: '',
            staticToken: '',
        });

        try {
            await api.getAlbums({ limit: 0 });
        } catch (e) {
            console.error('Invalid server:', e);
            throw e;
        }
    }

    return (
        <div>
            <p>What is your MoosicBox server's address?</p>
            <input
                ref={serverAddressInput!}
                type="text"
                placeholder="http://localhost:8000"
            />
            <button
                onClick={async () => {
                    await saveServerAddress();
                    window.location.href = '/';
                }}
                type="button"
                class="remove-button-styles finish-button"
            >
                Finish
            </button>
        </div>
    );
}
