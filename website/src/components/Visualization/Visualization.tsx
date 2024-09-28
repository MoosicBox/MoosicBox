import {
    createComputed,
    createEffect,
    createSignal,
    on,
    onCleanup,
    onMount,
} from 'solid-js';
import './Visualization.css';
import {
    currentSeek,
    currentTrackLength,
    offNextTrack,
    offPreviousTrack,
    offSeek,
    onSeek,
    playing as playerPlaying,
    playerState,
    seek,
} from '~/services/player';
import { toTime } from '~/services/formatting';
import { isServer } from 'solid-js/web';
import { api, trackId, type Track } from '~/services/api';

const VIZ_HEIGHT = 30;
const BAR_WIDTH = 2;
const BAR_GAP = 1;
const CURSOR_OFFSET = -1;
let visualizationData: number[] | undefined;
let mouseX: number;
let waitingForPlayback = true;
let targetPlaybackPos = 0;

function getTrackDuration() {
    return playerState.currentTrack?.duration ?? currentTrackLength();
}

function debounce(func: (e: Event) => void): (event: Event) => void {
    let timer: NodeJS.Timeout;
    return function (event: Event) {
        if (timer) clearTimeout(timer);
        timer = setTimeout(func, 300, event);
    };
}

function eventToSeekPosition(element: HTMLElement): number {
    if (!element) return 0;

    const pos = element.getBoundingClientRect()!;
    const percentage = Math.min(
        100,
        Math.max(0, (mouseX - pos.left) / pos.width),
    );
    return getTrackDuration() * percentage;
}

function seekTo(event: MouseEvent): void {
    const seekPos = Math.round(
        eventToSeekPosition(event.target as HTMLElement),
    );
    seek(seekPos, true);
    waitingForPlayback = true;
    targetPlaybackPos = seekPos;
}

let dragStartListener: (event: MouseEvent) => void;
let dragListener: (event: MouseEvent) => void;
let dragEndListener: (event: MouseEvent) => void;
let visibilityChangeListener: () => void;
let resizeListener: (event: Event) => void;
let onSeekListener: (seek: number, manual: boolean) => void;

enum BackToNowPlayingPosition {
    top = 'TOP',
    bottom = 'BOTTOM',
    none = 'NONE',
}

export default function player() {
    let canvas: HTMLCanvasElement;
    let progressBar: HTMLDivElement | undefined;
    let progressBarVisualizer: HTMLDivElement | undefined;
    let playlistSlideout: HTMLDivElement | undefined;
    let playlistSlideoutContentRef: HTMLDivElement | undefined;
    let backToNowPlayingTopRef: HTMLDivElement | undefined;
    let backToNowPlayingBottomRef: HTMLDivElement | undefined;
    let containerRef: HTMLDivElement | undefined;
    const [dragging, setDragging] = createSignal(false);
    const [applyDrag, setApplyDrag] = createSignal(false);
    const [seekPosition, setSeekPosition] = createSignal(currentSeek());
    const [playing, setPlaying] = createSignal(playerPlaying());

    const [data, setData] = createSignal<number[]>([]);
    const [lastCursor, setLastCursor] = createSignal<number>();
    const [lastDarken, setLastDarken] = createSignal(0);

    createComputed(() => {
        setPlaying(playerState.currentPlaybackSession?.playing ?? false);
    });

    function getSeekPosition(): number {
        return Math.max(Math.min(seekPosition() ?? 0, getTrackDuration()), 0);
    }

    function getCurrentSeekPosition(): number {
        return Math.max(Math.min(currentSeek() ?? 0, getTrackDuration()), 0);
    }

    function getProgressBarWidth(): number {
        if (applyDrag() && dragging()) {
            return getSeekPosition() / getTrackDuration();
        }

        return getCurrentSeekPosition() / getTrackDuration();
    }

    onMount(() => {
        if (!isServer) {
            dragStartListener = (event: MouseEvent) => {
                if (event.button === 0) {
                    progressBar?.classList.add('no-transition');
                    setDragging(true);
                    setApplyDrag(true);
                }
            };
            dragListener = (event: MouseEvent) => {
                mouseX = event.clientX;
                if (dragging()) {
                    event.preventDefault();
                    if (!applyDrag()) return;
                }
                setSeekPosition(eventToSeekPosition(progressBarVisualizer!));
            };
            dragEndListener = (event: MouseEvent) => {
                if (event.button === 0 && dragging()) {
                    setDragging(false);
                    if (!applyDrag()) return;
                    setApplyDrag(false);
                    seekTo(event);
                    progressBar?.classList.remove('no-transition');
                    event.preventDefault();
                }
            };

            visibilityChangeListener = () => {
                if (document.visibilityState !== 'hidden') {
                    animationStart = undefined;
                }
            };

            resizeListener = debounce(async () => {
                if (playerState.currentTrack) {
                    await loadVisualizationData(playerState.currentTrack);
                    initVisualization();
                }
            });

            progressBarVisualizer?.addEventListener(
                'mousedown',
                dragStartListener,
            );
            window.addEventListener('mousemove', dragListener);
            window.addEventListener('mouseup', dragEndListener);
            document.addEventListener(
                'visibilitychange',
                visibilityChangeListener,
            );
            window.addEventListener('resize', resizeListener);

            onSeekListener = (_seek, manual) => {
                if (manual) {
                    if (!visualizationData) return;
                    initVisualization();
                }
            };
            onSeek(onSeekListener);
        }
    });

    onCleanup(() => {
        if (!isServer) {
            progressBarVisualizer?.removeEventListener(
                'mousedown',
                dragStartListener,
            );
            window.removeEventListener('mousemove', dragListener);
            window.removeEventListener('mouseup', dragEndListener);
            document.removeEventListener(
                'visibilitychange',
                visibilityChangeListener,
            );
            window.removeEventListener('resize', resizeListener);
            offSeek(onSeekListener);
        }
    });

    createEffect(
        on(
            () => currentSeek(),
            (value) => {
                animationStart = document.timeline.currentTime as number;
                if (
                    waitingForPlayback &&
                    (value ?? 0) > targetPlaybackPos &&
                    (targetPlaybackPos === 0 ||
                        (value ?? 0) <= targetPlaybackPos + 1) &&
                    playing()
                ) {
                    console.debug('playback started');
                    waitingForPlayback = false;
                    animationStart = undefined;
                    startAnimation();
                }
            },
        ),
    );

    function initVisualization() {
        if (!visualizationData) {
            throw new Error('No visualizationData set');
        }

        setLastDarken(0);

        const ratio = window.devicePixelRatio;
        canvas.width = window.innerWidth * ratio;
        canvas.height = VIZ_HEIGHT * ratio;

        const ctx = canvas.getContext('2d')!;
        ctx.scale(ratio, ratio);
        ctx.fillStyle = 'white';

        const delta = Math.max(
            1,
            (visualizationData.length / window.innerWidth) * 2,
        );

        const sizedData: number[] = [];

        for (
            let i = 0;
            i < visualizationData.length &&
            sizedData.length < window.innerWidth;
            i += delta
        ) {
            sizedData.push(visualizationData[~~i]!);
        }

        setData(sizedData);

        const cursor = getProgressBarWidth();
        ctx.fillStyle = 'white';
        drawVisualizationPoints(0, data().length);
        darkenVisualization(cursor);
        drawVisualization(cursor);
    }

    function darkenVisualization(cursor: number) {
        const ctx = canvas.getContext('2d')!;
        const lastDarkenValue = lastDarken();

        const points = data();
        const darken = ~~(cursor * points.length);

        if (lastDarkenValue > darken) {
            ctx.fillStyle = 'white';
            drawVisualizationPoints(darken + 1, lastDarkenValue);
        }

        ctx.fillStyle = '#222222';
        drawVisualizationPoints(Math.max(0, lastDarkenValue - 1), darken + 1);
        setLastDarken(darken);
    }

    function clearVisualization(cursor: number) {
        const ctx = canvas.getContext('2d')!;
        ctx.clearRect(0, 0, canvas.width, canvas.height);
        drawVisualization(cursor);
    }

    function drawVisualization(cursor: number) {
        const ctx = canvas.getContext('2d')!;
        const ratio = window.devicePixelRatio;
        const lastCursorValue = lastCursor();

        if (typeof lastCursorValue === 'number') {
            const start = lastCursorValue;
            const end = lastCursorValue;
            const paintStart = ~~((start * canvas.width) / ratio) - 1;
            const paintEnd = Math.ceil((end * canvas.width) / ratio) + 3;
            ctx.clearRect(paintStart, 0, paintEnd - paintStart, canvas.height);

            const points = data();
            const paintStartI = ~~(start * points.length) - 2;
            const paintEndI = Math.ceil(end * points.length) + 2;

            darkenVisualization(cursor);
            const lastDarkenValue = lastDarken();
            ctx.fillStyle = 'white';
            drawVisualizationPoints(
                Math.max(lastDarkenValue + 1, paintStartI),
                paintEndI,
            );
        } else {
            darkenVisualization(cursor);
        }

        ctx.fillStyle = 'white';
        ctx.fillRect(
            (canvas.width * cursor) / ratio + CURSOR_OFFSET,
            0,
            2,
            VIZ_HEIGHT,
        );
        setLastCursor(cursor);
    }

    function drawVisualizationPoints(start: number, end: number) {
        if (start >= end) return;

        const ctx = canvas.getContext('2d')!;
        const points = data();

        ctx.clearRect(
            start * (BAR_GAP + BAR_WIDTH) - 0.5 + CURSOR_OFFSET,
            0,
            (end - start) * (BAR_GAP + BAR_WIDTH) - 0.5,
            VIZ_HEIGHT,
        );

        for (let i = start; i < end; i++) {
            const point = points[i]!;
            ctx.fillRect(
                i * (BAR_GAP + BAR_WIDTH) + CURSOR_OFFSET,
                VIZ_HEIGHT / 2 - point / 2,
                BAR_WIDTH / 2,
                point,
            );
        }
    }

    async function loadVisualizationData(track: Track): Promise<void> {
        const max = window.innerWidth / (BAR_GAP + BAR_WIDTH);
        const data: number[] = await api.getTrackVisualization(
            track,
            track.type,
            max,
        );
        data.forEach((x, i) => {
            data[i] = Math.max(3, Math.round((x / 255) * VIZ_HEIGHT));
        });
        visualizationData = data;
    }

    createEffect(
        on(
            () => playerState.currentTrack,
            async (track, prev) => {
                if (prev && track && trackId(prev) === trackId(track)) return;
                visualizationData = undefined;
                setData([]);
                const cursor = getProgressBarWidth();
                clearVisualization(cursor);

                waitingForPlayback = true;
                targetPlaybackPos = 0;

                if (track) {
                    await loadVisualizationData(track);
                    initVisualization();
                }
            },
        ),
    );

    createEffect(
        on(
            () => currentTrackLength(),
            () => {
                setSeekPosition(eventToSeekPosition(progressBarVisualizer!));
                updateVisualizationBarOpacity();
            },
        ),
    );

    createEffect(
        on(
            () => playing(),
            (playing) => {
                if (!playing) {
                    waitingForPlayback = true;
                }
                if (dragging()) {
                    setApplyDrag(false);
                    progressBar?.classList.remove('no-transition');
                }
            },
        ),
    );

    let nextTrackListener: () => void;
    let previousTrackListener: () => void;

    onCleanup(() => {
        offNextTrack(nextTrackListener);
        offPreviousTrack(previousTrackListener);
    });

    const [backToNowPlayingPosition, setBackToNowPlayingPosition] =
        createSignal(BackToNowPlayingPosition.none);

    let backToNowPlayingTopTimeout: NodeJS.Timeout;
    let backToNowPlayingBottomTimeout: NodeJS.Timeout;
    const scrollListener = () => {
        if (!getCurrentTrack()) return;

        if (
            getCurrentTrack()!.getBoundingClientRect().top >
            playlistSlideout!.offsetHeight
        ) {
            clearTimeout(backToNowPlayingBottomTimeout);
            setBackToNowPlayingPosition(BackToNowPlayingPosition.bottom);
            backToNowPlayingTopRef!.style.opacity = '0';
            backToNowPlayingBottomRef!.style.display = 'block';
            setTimeout(() => {
                backToNowPlayingBottomRef!.style.opacity = '1';
            }, 0);
        } else if (getCurrentTrack()!.getBoundingClientRect().bottom < 0) {
            clearTimeout(backToNowPlayingTopTimeout);
            setBackToNowPlayingPosition(BackToNowPlayingPosition.top);
            backToNowPlayingBottomRef!.style.opacity = '0';
            backToNowPlayingTopRef!.style.display = 'block';
            setTimeout(() => {
                backToNowPlayingTopRef!.style.opacity = '1';
            }, 0);
        } else {
            backToNowPlayingTopRef!.style.opacity = '0';
            backToNowPlayingBottomRef!.style.opacity = '0';
            if (backToNowPlayingPosition() === BackToNowPlayingPosition.top) {
                backToNowPlayingTopTimeout = setTimeout(() => {
                    backToNowPlayingTopRef!.style.display = 'none';
                }, 300);
            } else if (
                backToNowPlayingPosition() === BackToNowPlayingPosition.bottom
            ) {
                backToNowPlayingBottomTimeout = setTimeout(() => {
                    backToNowPlayingBottomRef!.style.display = 'none';
                }, 300);
            }
            setBackToNowPlayingPosition(BackToNowPlayingPosition.none);
        }
    };

    onMount(() => {
        if (isServer) return;
        playlistSlideoutContentRef?.addEventListener('scroll', scrollListener);

        scrollListener();
    });

    onCleanup(() => {
        if (isServer) return;
        playlistSlideoutContentRef?.removeEventListener(
            'scroll',
            scrollListener,
        );
    });

    function getCurrentTrack(): Element | null {
        return (
            playlistSlideout?.querySelector('.playlist-tracks-track.current') ??
            null
        );
    }

    let animationStart: number | undefined;

    function progressAnimationFrame(ts: number): void {
        if (!animationStart) animationStart = ts;

        const elapsed = ts - animationStart;

        const duration = getTrackDuration();

        if (
            typeof currentSeek() !== 'undefined' &&
            typeof duration !== 'undefined'
        ) {
            const offset = (elapsed / 1000) * (1 / duration);

            updateVisualizationBarOpacity(offset);
        }

        if (!playing() || waitingForPlayback) {
            animationStart = undefined;
            console.debug('Stopping animation');

            return;
        }

        window.requestAnimationFrame(progressAnimationFrame);
    }

    function startAnimation() {
        window.requestAnimationFrame((ts) => {
            animationStart = ts;
            window.requestAnimationFrame(progressAnimationFrame);
        });
    }

    function updateVisualizationBarOpacity(offset: number = 0) {
        if (waitingForPlayback) {
            return;
        }

        const cursor = getProgressBarWidth() + offset;
        drawVisualization(cursor);
    }

    return (
        <>
            <div ref={containerRef!} class="visualization">
                <div class="visualization-media-controls-seeker-bar">
                    <div
                        ref={progressBarVisualizer!}
                        class="visualization-media-controls-seeker-bar-progress-trigger visualization-media-controls-seeker-bar-visualizer"
                        style={{
                            top: `-${Math.round(VIZ_HEIGHT / 2) - 2}px`,
                            height: `${VIZ_HEIGHT}px`,
                        }}
                    >
                        <canvas
                            ref={canvas!}
                            class="visualization-canvas"
                            width="0"
                            height={VIZ_HEIGHT}
                            style={{ height: `${VIZ_HEIGHT}px` }}
                        ></canvas>
                    </div>
                    <div
                        class="visualization-media-controls-seeker-bar-progress-tooltip"
                        style={{
                            left: `max(30px, min(100vw - 40px, ${
                                (getSeekPosition() / getTrackDuration()) * 100
                            }%))`,
                            display:
                                applyDrag() && dragging() ? 'block' : undefined,
                        }}
                    >
                        {toTime(Math.round(getSeekPosition()))}
                    </div>
                </div>
            </div>
        </>
    );
}
