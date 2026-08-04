const CSRF_META_NAME = 'hyperchad-shared-state-csrf';
const CSRF_COOKIE_META_NAME = 'hyperchad-shared-state-csrf-cookie';
const CSRF_HEADER_NAME = 'x-hyperchad-csrf-token';
const CSRF_SOURCE_HEADER_NAME = 'x-hyperchad-csrf-source';
const CSRF_COOKIE_COUNT_HEADER_NAME = 'x-hyperchad-csrf-cookie-count';
const CSRF_META_MATCH_HEADER_NAME = 'x-hyperchad-csrf-meta-match';

export type CsrfDiagnostics = {
    token: string | null;
    source: 'cookie' | 'meta' | 'missing';
    cookieCount: number;
    metaMatches: boolean | null;
};

function csrfMetaToken(): string | null {
    return (
        document
            .querySelector<HTMLMetaElement>(`meta[name="${CSRF_META_NAME}"]`)
            ?.getAttribute('content') ?? null
    );
}

function csrfCookieTokens(): string[] {
    const cookieName = document
        .querySelector<HTMLMetaElement>(`meta[name="${CSRF_COOKIE_META_NAME}"]`)
        ?.getAttribute('content');
    if (!cookieName) {
        return [];
    }

    const tokens: string[] = [];
    for (const cookie of document.cookie.split(';')) {
        const separator = cookie.indexOf('=');
        if (separator === -1) {
            continue;
        }
        if (cookie.slice(0, separator).trim() === cookieName) {
            tokens.push(cookie.slice(separator + 1));
        }
    }
    return tokens;
}

export function csrfDiagnostics(): CsrfDiagnostics {
    const metaToken = csrfMetaToken();
    const cookieTokens = csrfCookieTokens();
    const cookieToken = cookieTokens[0] ?? null;
    return {
        token: cookieToken ?? metaToken,
        source: cookieToken ? 'cookie' : metaToken ? 'meta' : 'missing',
        cookieCount: cookieTokens.length,
        metaMatches:
            cookieToken && metaToken ? cookieToken === metaToken : null,
    };
}

export function csrfToken(): string | null {
    return csrfDiagnostics().token;
}

export function withCsrfHeader(headers: HeadersInit = {}): Headers {
    const result = new Headers(headers);
    const diagnostics = csrfDiagnostics();
    if (diagnostics.token) {
        result.set(CSRF_HEADER_NAME, diagnostics.token);
    }
    result.set(CSRF_SOURCE_HEADER_NAME, diagnostics.source);
    result.set(
        CSRF_COOKIE_COUNT_HEADER_NAME,
        diagnostics.cookieCount.toString(),
    );
    result.set(
        CSRF_META_MATCH_HEADER_NAME,
        diagnostics.metaMatches === null
            ? 'unknown'
            : diagnostics.metaMatches.toString(),
    );
    return result;
}
