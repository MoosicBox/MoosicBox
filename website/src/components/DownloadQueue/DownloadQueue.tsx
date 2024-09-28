import Album from '~/components/Album';
import {
    For,
    Show,
    createComputed,
    createEffect,
    createSignal,
    on,
} from 'solid-js';
import type { JSXElement } from 'solid-js';
import { api, Api } from '~/services/api';
import type {
    ApiSource,
    Track as ApiTrack,
    Album as ApiAlbum,
    Artist as ApiArtist,
} from '~/services/api';
import { downloadsState } from '~/services/downloads';
import {
    displayApiSource,
    displayDownloadTaskState,
    downloadTaskStateClassName,
} from '~/services/formatting';
import Artist from '~/components/Artist';

function downloadTaskProgress(task: Api.DownloadTask): JSXElement {
    return (
        <>
            <div class="downloads-download-task-progress-details">
                {typeof task.bytes === 'number' &&
                typeof task.totalBytes === 'number'
                    ? `${(task.bytes / 1024 / 1024).toFixed(2)}/${(
                          task.totalBytes /
                          1024 /
                          1024
                      ).toFixed(2)} MiB - `
                    : ''}
                {~~task.progress}%
                {task.speed ? ` - ${(task.speed / 1024).toFixed(2)} KiB/s` : ''}
            </div>
            <div class="downloads-download-task-progress-bar">
                <div
                    class="downloads-download-task-progress-bar-progress"
                    style={{
                        width: `${task.progress}%`,
                    }}
                ></div>
                <div class="downloads-download-task-progress-bar-progress-trigger"></div>
            </div>
        </>
    );
}

function downloadTask(task: Api.DownloadTask): JSXElement {
    const id = task.id;
    const item = task.item;
    const taskType = item.type;

    switch (taskType) {
        case 'TRACK': {
            return (
                <>
                    <div class="downloads-download-task-cover">
                        <Album
                            album={
                                {
                                    ...task.item,
                                    type: item.source,
                                } as unknown as ApiTrack
                            }
                            size={80}
                        />
                    </div>
                    <div class="downloads-download-task-details">
                        <div class="downloads-download-task-header-details">
                            Track ({item.trackId}) - {item.title} -{' '}
                            {displayDownloadTaskState(task.state)} -{' '}
                            {displayApiSource(item.source as ApiSource)}
                            <Show when={task.state === 'ERROR'}>
                                <button onClick={() => api.retryDownload(id)}>
                                    Retry
                                </button>
                            </Show>
                        </div>
                        <div class="downloads-download-task-location-details">
                            {task.filePath}
                        </div>
                        <div class="downloads-download-task-progress">
                            <Show when={task.state === 'STARTED'}>
                                {downloadTaskProgress(task)}
                            </Show>
                        </div>
                    </div>
                </>
            );
        }
        case 'ALBUM_COVER': {
            return (
                <>
                    <div class="downloads-download-task-cover">
                        <Album
                            album={
                                {
                                    ...task.item,
                                    type: item.source,
                                } as unknown as ApiAlbum
                            }
                            size={80}
                        />
                    </div>
                    <div class="downloads-download-task-details">
                        <div class="downloads-download-task-header-details">
                            Album ({item.albumId}) cover - {item.title} -{' '}
                            {displayDownloadTaskState(task.state)}
                            <Show when={task.state === 'ERROR'}>
                                <button onClick={() => api.retryDownload(id)}>
                                    Retry
                                </button>
                            </Show>
                        </div>
                        <div class="downloads-download-task-location-details">
                            {task.filePath}
                        </div>
                        <div class="downloads-download-task-progress">
                            <Show when={task.state === 'STARTED'}>
                                {downloadTaskProgress(task)}
                            </Show>
                        </div>
                    </div>
                </>
            );
        }
        case 'ARTIST_COVER': {
            return (
                <>
                    <div class="downloads-download-task-cover">
                        <Artist
                            artist={
                                {
                                    ...task.item,
                                    type: item.source,
                                } as unknown as ApiArtist
                            }
                            size={80}
                        />
                    </div>
                    <div class="downloads-download-task-details">
                        <div class="downloads-download-task-header-details">
                            Artist ({item.artistId}) (album_id: {item.albumId})
                            cover - {item.title} -{' '}
                            {displayDownloadTaskState(task.state)}
                            <Show when={task.state === 'ERROR'}>
                                <button onClick={() => api.retryDownload(id)}>
                                    Retry
                                </button>
                            </Show>
                        </div>
                        <div class="downloads-download-task-location-details">
                            {task.filePath}
                        </div>
                        <div class="downloads-download-task-progress">
                            <Show when={task.state === 'STARTED'}>
                                {downloadTaskProgress(task)}
                            </Show>
                        </div>
                    </div>
                </>
            );
        }
        default:
            taskType satisfies never;
            throw new Error(`Invalid taskType: '${taskType}'`);
    }
}

export type DownloadQueueState = 'QUEUED' | 'HISTORY';

export default function downloadsPage(props: { state: DownloadQueueState }) {
    const [tasks, setTasks] = createSignal<Api.DownloadTask[]>([]);

    function initTasks() {
        const state = props.state;

        switch (state) {
            case 'QUEUED':
                setTasks(downloadsState.currentTasks);
                break;
            case 'HISTORY':
                setTasks(downloadsState.historyTasks);
                break;
            default:
                state satisfies never;
                throw new Error(`Invalid DownloadQueueState '${state}'`);
        }
    }

    createComputed(initTasks);
    createEffect(on(() => downloadsState.tasks, initTasks));

    return (
        <>
            {tasks().length === 0 ? (
                <div class="downloads-download-tasks">
                    No {props.state === 'QUEUED' ? 'queued' : 'history'} tasks
                </div>
            ) : (
                <div class="downloads-download-tasks">
                    <For each={tasks()}>
                        {(task) => (
                            <div
                                class={`downloads-download-task ${downloadTaskStateClassName(task.state)}`}
                            >
                                {downloadTask(task)}
                            </div>
                        )}
                    </For>
                </div>
            )}
        </>
    );
}
