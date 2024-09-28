import { Show, createSignal, onMount } from 'solid-js';
import {
    api,
    connection,
    getNewConnectionId,
    setConnection,
    type Connection,
} from '~/services/api';
import { getQueryParam } from '~/services/util';

export default function authPage() {
    const magicTokenParam = getQueryParam('magicToken');
    const apiUrlParam = getQueryParam('apiUrl');

    const [loading, setLoading] = createSignal(true);
    const [error, setError] = createSignal<string>();

    async function saveConnection(values: Partial<Connection>) {
        const con = connection.get();
        const id = con?.id ?? getNewConnectionId();
        await setConnection(id, values);
    }

    onMount(async () => {
        if (!magicTokenParam) {
            setLoading(false);
            setError('No magic token');
            return;
        }

        if (apiUrlParam) {
            await saveConnection({
                apiUrl: apiUrlParam,
            });
        }

        const resp = await api.magicToken(magicTokenParam);
        setLoading(false);

        if (resp) {
            await saveConnection({
                clientId: resp.clientId,
                token: resp.accessToken,
            });
            window.location.href = '/';
        } else {
            setError('Failed to authenticate with magic token');
        }
    });

    return (
        <div>
            {loading() ? (
                <>Loading...</>
            ) : (
                <>
                    <Show when={error()}>{error()}</Show>
                </>
            )}
        </div>
    );
}
