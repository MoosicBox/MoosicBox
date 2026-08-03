const CSRF_META_NAME = 'hyperchad-shared-state-csrf';
const CSRF_COOKIE_META_NAME = 'hyperchad-shared-state-csrf-cookie';
const CSRF_HEADER_NAME = 'x-hyperchad-csrf-token';

function csrfCookieToken(): string | null {
    const cookieName = document
        .querySelector<HTMLMetaElement>(`meta[name="${CSRF_COOKIE_META_NAME}"]`)
        ?.getAttribute('content');
    if (!cookieName) {
        return null;
    }

    for (const cookie of document.cookie.split(';')) {
        const separator = cookie.indexOf('=');
        if (separator === -1) {
            continue;
        }
        if (cookie.slice(0, separator).trim() === cookieName) {
            return cookie.slice(separator + 1);
        }
    }

    return null;
}

export function csrfToken(): string | null {
    return (
        csrfCookieToken() ??
        document
            .querySelector<HTMLMetaElement>(`meta[name="${CSRF_META_NAME}"]`)
            ?.getAttribute('content') ??
        null
    );
}

export function withCsrfHeader(headers: HeadersInit = {}): Headers {
    const result = new Headers(headers);
    const token = csrfToken();
    if (token) {
        result.set(CSRF_HEADER_NAME, token);
    }
    return result;
}
