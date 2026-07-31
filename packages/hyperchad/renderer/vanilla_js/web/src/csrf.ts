const CSRF_META_NAME = 'hyperchad-shared-state-csrf';
const CSRF_HEADER_NAME = 'x-hyperchad-csrf-token';

export function csrfToken(): string | null {
    return (
        document
            .querySelector<HTMLMetaElement>(`meta[name="${CSRF_META_NAME}"]`)
            ?.getAttribute('content') ?? null
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
