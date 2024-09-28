import { createStore, produce } from 'solid-js/store';
import { Api, api } from './api';
import { createListener } from './util';
import { onStartup } from './app';

export type DownloadEventType =
    | BytesReadDownloadEvent['type']
    | SizeDownloadEvent['type']
    | SpeedDownloadEvent['type']
    | StateDownloadEvent['type'];

export type BytesReadDownloadEvent = {
    taskId: number;
    read: number;
    total: number;
    type: 'BYTES_READ';
};

export type SizeDownloadEvent = {
    taskId: number;
    bytes?: number;
    type: 'SIZE';
};

export type SpeedDownloadEvent = {
    taskId: number;
    bytesPerSecond: number;
    type: 'SPEED';
};

export type StateDownloadEvent = {
    taskId: number;
    state: Api.DownloadTaskState;
    type: 'STATE';
};

export type DownloadEvent = (
    | BytesReadDownloadEvent
    | SizeDownloadEvent
    | SpeedDownloadEvent
    | StateDownloadEvent
) & { type: DownloadEventType };

export const onDownloadEventListener =
    createListener<(value: DownloadEvent) => boolean | void>();
export const onDownloadEvent = onDownloadEventListener.on;
export const offDownloadEvent = onDownloadEventListener.off;

interface DownloadsState {
    tasks: Api.DownloadTask[];
    currentTasks: Api.DownloadTask[];
    historyTasks: Api.DownloadTask[];
}

export const [downloadsState, setDownloadsState] = createStore<DownloadsState>({
    tasks: [],
    currentTasks: [],
    historyTasks: [],
});

function handleDownloadEvent(event: DownloadEvent) {
    const eventType = event.type;

    switch (eventType) {
        case 'SIZE':
            setDownloadsState(
                produce((state) => {
                    const task = state.tasks.find(
                        (task) => task.id === event.taskId,
                    );
                    if (task) {
                        task.totalBytes = event.bytes ?? task.totalBytes;
                    }
                }),
            );
            break;
        case 'BYTES_READ':
            setDownloadsState(
                produce((state) => {
                    const task = state.tasks.find(
                        (task) => task.id === event.taskId,
                    );
                    if (task) {
                        task.bytes = event.total;
                        task.progress = (event.total / task.totalBytes) * 100;
                    }
                }),
            );
            break;
        case 'SPEED':
            setDownloadsState(
                produce((state) => {
                    const task = state.tasks.find(
                        (task) => task.id === event.taskId,
                    );
                    if (task) {
                        task.speed = event.bytesPerSecond;
                    }
                }),
            );
            break;
        case 'STATE':
            setDownloadsState(
                produce((state) => {
                    const task = state.tasks.find(
                        (task) => task.id === event.taskId,
                    );
                    if (task) {
                        const prevState = task.state;
                        task.state = event.state;

                        if (isCurrent(task.state) && !isCurrent(prevState)) {
                            const index = state.historyTasks.indexOf(task);
                            if (index !== -1) {
                                state.historyTasks.splice(index, 1);
                            }
                            state.currentTasks.unshift(task);
                        } else if (
                            !isCurrent(task.state) &&
                            isCurrent(prevState)
                        ) {
                            const index = state.currentTasks.indexOf(task);
                            if (index !== -1) {
                                state.currentTasks.splice(index, 1);
                            }
                            state.historyTasks.unshift(task);
                        }

                        if (task.state === 'FINISHED') {
                            task.progress = 100;
                        }
                    }
                }),
            );
            break;
        default:
            eventType satisfies never;
            throw new Error(`Invalid DownloadEvent type: '${eventType}'`);
    }
}

onDownloadEvent(handleDownloadEvent);

function isCurrent(state: Api.DownloadTaskState): boolean {
    return state === 'STARTED' || state === 'PAUSED' || state === 'PENDING';
}

function isHistorical(state: Api.DownloadTaskState): boolean {
    return !isCurrent(state);
}

onStartup(async () => {
    const tasks = await api.getDownloadTasks();

    const current = tasks.items.filter(({ state }) => isCurrent(state));
    const history = tasks.items.filter(({ state }) => isHistorical(state));

    setDownloadsState(
        produce((state) => {
            state.tasks = tasks.items;
            state.currentTasks = current;
            state.historyTasks = history;
        }),
    );
});
