import { QueryParams, os, objToStr } from './util';

class RequestError extends Error {
    constructor(public response: Response) {
        let message = `Request failed: ${response.status}`;

        if (response.statusText) {
            message += ` (${response.statusText})`;
        }

        if (response.url) {
            message += ` (url='${response.url}')`;
        }

        if (typeof response.redirected !== 'undefined') {
            message += ` (redirected=${response.redirected})`;
        }

        if (response.headers) {
            message += ` (headers=${objToStr(response.headers)})`;
        }

        if (response.type) {
            message += ` (type=${response.type})`;
        }

        super(message);
    }
}

export interface GitHubRelease {
    name: string;
    html_url: string;
    created_at: string;
    published_at: string;
    assets: Asset[];
}

export interface Asset {
    browser_download_url: string;
    name: string;
    size: number;
}

export interface OsRelease {
    assets: OsAsset[];
    version: string;
    url: string;
    createdAt: string;
    publishedAt: string;
}

export interface OsAsset {
    name: string;
    assetMatcher: (assetName: string) => boolean;
    asset?: Asset | undefined;
    showMoreFormats: boolean;
    otherFormats: Asset[];
}

export async function getGitHubReleases(opts: {
    org: string;
    repo: string;
}): Promise<GitHubRelease[]> {
    const apiRoot = `https://api.github.com/repos/${opts.org}/${opts.repo}`;
    const releaseUrl = `${apiRoot}/releases`;

    const releases = await requestJson<GitHubRelease[]>(releaseUrl);

    releases.sort((a, b) => b.published_at.localeCompare(a.published_at));

    return releases;
}

function createAsset(
    release: GitHubRelease,
    name: string,
    assetName: string,
    assetMatcher: (assetName: string) => boolean,
    assetNotMatcher?: (assetName: string) => boolean,
): OsAsset {
    const { assets } = release;

    const otherFormats = assets.filter(
        (a) =>
            a.name !== assetName &&
            assetMatcher(a.name) &&
            (!assetNotMatcher || !assetNotMatcher(a.name)),
    );
    const asset =
        assets.find((a) => a.name === assetName) ?? otherFormats.shift();

    return {
        name,
        assetMatcher,
        asset,
        showMoreFormats: true,
        otherFormats,
    };
}

export function createOsRelease(release: GitHubRelease): OsRelease {
    const { name, html_url } = release;

    function matches(value: RegExp): (name: string) => boolean {
        return (name) => name.match(value) !== null;
    }

    const mac_intel_matches = matches(
        /(.+?(x64|aarch64).*?\.dmg|.+?_macos_x64.*|.+?_x64_macos.*)/gi,
    );

    const value: OsRelease = {
        assets: [
            createAsset(
                release,
                'windows',
                'MoosicBox_x64.msi',
                matches(/(.+?\.msi|.+?\.exe)/gi),
            ),
            createAsset(
                release,
                'mac_apple_silicon',
                'MoosicBox.dmg',
                matches(/(.+?\.dmg|.+?_macos.*)/gi),
                mac_intel_matches,
            ),
            createAsset(
                release,
                'mac_intel',
                'MoosicBox_x64.dmg',
                mac_intel_matches,
            ),
            createAsset(
                release,
                'linux',
                'moosicbox_amd64.deb',
                matches(/(.+?\.AppImage|.+?\.deb|.+?_linux.*)/gi),
            ),
            createAsset(
                release,
                'android',
                'MoosicBox.apk',
                matches(/(.+?\.aab|.+?\.apk)/gi),
            ),
        ],
        version: name,
        url: html_url,
        createdAt: release.created_at,
        publishedAt: release.published_at,
    };

    value.assets.sort((a, b) => {
        if (os.lowerName === a.name) {
            return -1;
        } else if (os.lowerName === b.name) {
            return 1;
        } else {
            return 0;
        }
    });

    value.assets.sort((a, b) => {
        if (a.name === 'mac_intel' && b.name !== 'mac_apple_silicon') {
            return -1;
        } else if (b.name === 'mac_intel' && a.name !== 'mac_apple_silicon') {
            return 1;
        } else {
            return 0;
        }
    });

    return value;
}

async function requestJson<T>(
    url: string,
    options?: Parameters<typeof fetch>[1],
): Promise<T> {
    if (url[url.length - 1] === '?') url = url.substring(0, url.length - 1);

    const params = new QueryParams();

    if (params.size > 0) {
        if (url.indexOf('?') > 0) {
            url += `&${params}`;
        } else {
            url += `?${params}`;
        }
    }

    const response = await fetch(url, options);

    if (!response.ok) {
        throw new RequestError(response);
    }

    return await response.json();
}

export function cancellable<T>(func: (signal: AbortSignal) => Promise<T>): {
    data: Promise<T>;
    controller: AbortController;
    signal: AbortSignal;
} {
    const controller = new AbortController();
    const signal = controller.signal;

    return { data: func(signal), controller, signal };
}

const abortControllers: { [id: string]: AbortController } = {};

export async function once<T>(
    id: string,
    func: (signal: AbortSignal) => Promise<T>,
): Promise<T> {
    const controller = abortControllers[id];

    if (controller) {
        controller.abort();
    }

    const resp = cancellable(func);
    abortControllers[id] = resp.controller;

    let data: T;

    try {
        data = await resp.data;
    } catch (e) {
        throw e;
    } finally {
        delete abortControllers[id];
    }

    return data;
}
