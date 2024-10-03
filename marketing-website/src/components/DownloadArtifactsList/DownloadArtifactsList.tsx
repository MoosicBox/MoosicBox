import { For, Show, createSignal, onMount } from 'solid-js';
import './download-artifacts-list.css';
import {
    createOsRelease,
    getGitHubReleases,
    type OsRelease,
} from '~/services/api';
import { displayDate, formatClassName, getSize } from '~/services/formatting';
import { getOsHeader, os } from '~/services/util';

export default function modalFunc() {
    const [releases, setReleases] = createSignal<OsRelease[]>([]);

    onMount(async () => {
        await Promise.all([
            (async () => {
                const newReleases = await getGitHubReleases({
                    org: 'MoosicBox',
                    repo: 'MoosicBox',
                });
                setReleases([
                    ...newReleases.map(createOsRelease),
                    ...(releases() ?? []),
                ]);
            })(),
            (async () => {
                const archivedReleases = await getGitHubReleases({
                    org: 'MoosicBox',
                    repo: 'MoosicBoxApp',
                });
                setReleases([
                    ...(releases() ?? []),
                    ...archivedReleases.map(createOsRelease),
                ]);
            })(),
        ]);
    });

    return (
        <div class="download-artifacts-list">
            <Show when={releases()}>
                <For each={releases()}>
                    {(release) => (
                        <div
                            class="download-artifacts-list-release"
                            id={formatClassName(release.version)}
                        >
                            <h2 class="download-artifacts-list-release-header">
                                <div>Release {release.version}</div>
                                <div class="download-artifacts-list-release-header-date">
                                    {displayDate(
                                        release.publishedAt,
                                        'LLLL dd, yyyy HH:mm:ss',
                                    )}
                                </div>
                                <div class="download-artifacts-list-release-header-github">
                                    [
                                    <a target="_blank" href={release.url}>
                                        GitHub
                                    </a>
                                    ]
                                </div>
                            </h2>
                            <For each={release.assets}>
                                {(releaseAsset) => (
                                    <Show when={releaseAsset.asset}>
                                        {(asset) => (
                                            <div class="download-artifacts-list-release-os">
                                                <Show
                                                    when={
                                                        os.lowerName ===
                                                        releaseAsset.name
                                                    }
                                                >
                                                    <div class="download-artifacts-list-release-os-comment">
                                                        // We think you are
                                                        running {os.header}
                                                    </div>
                                                </Show>
                                                <h3 class="download-artifacts-list-release-os-header">
                                                    {getOsHeader(
                                                        releaseAsset.name,
                                                    )}
                                                </h3>
                                                Download{' '}
                                                <a
                                                    href={
                                                        asset()
                                                            .browser_download_url
                                                    }
                                                >
                                                    {asset().name}
                                                </a>{' '}
                                                <span class="download-artifacts-list-release-asset-size">
                                                    ({getSize(asset().size)}){' '}
                                                </span>
                                                <ul class="download-artifacts-list-other-assets">
                                                    <For
                                                        each={
                                                            releaseAsset.otherFormats
                                                        }
                                                    >
                                                        {(otherAsset) => (
                                                            <li>
                                                                <a
                                                                    href={
                                                                        otherAsset.browser_download_url
                                                                    }
                                                                >
                                                                    {
                                                                        otherAsset.name
                                                                    }
                                                                </a>{' '}
                                                                <span class="download-artifacts-list-release-asset-size">
                                                                    (
                                                                    {getSize(
                                                                        otherAsset.size,
                                                                    )}
                                                                    )
                                                                </span>
                                                            </li>
                                                        )}
                                                    </For>
                                                </ul>
                                            </div>
                                        )}
                                    </Show>
                                )}
                            </For>
                        </div>
                    )}
                </For>
            </Show>
        </div>
    );
}
