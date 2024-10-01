import { format, parseISO } from 'date-fns';
import { Api, type ApiSource } from './api';

function zeroPad(num: number, places: number) {
    return String(num).padStart(places, '0');
}

export function toTime(value: number) {
    const seconds = Math.round(value);

    const minutes = ~~(seconds / 60);
    const minutesAndSeconds = `${minutes % 60}:${zeroPad(seconds % 60, 2)}`;

    if (minutes >= 60) {
        const pad = minutes % 60 < 10 ? '0' : '';
        return `${~~(minutes / 60)}:${pad}${minutesAndSeconds}`;
    }

    return minutesAndSeconds;
}

export function displayDate(date: string, dateFormat: string): string {
    if (!date) return '';
    return format(parseISO(date), dateFormat);
}

export function displayAlbumVersionQuality(
    version: Api.AlbumVersionQuality,
): string {
    let str = '';

    switch (version.source) {
        case Api.TrackSource.LOCAL:
            break;
        case Api.TrackSource.TIDAL:
            str += 'Tidal';
            break;
        case Api.TrackSource.QOBUZ:
            str += 'Qobuz';
            break;
        case Api.TrackSource.YT:
            str += 'YouTube Music';
            break;
        default:
            version.source satisfies never;
    }

    if (version.format) {
        if (str.length > 0) {
            str += ' ';
        }
        switch (version.format) {
            case Api.AudioFormat.AAC:
                str += 'AAC';
                break;
            case Api.AudioFormat.FLAC:
                str += 'FLAC';
                break;
            case Api.AudioFormat.MP3:
                str += 'MP3';
                break;
            case Api.AudioFormat.OPUS:
                str += 'OPUS';
                break;
            case Api.AudioFormat.SOURCE:
                break;
            default:
                version.format satisfies never;
        }
    }
    if (version.sampleRate) {
        if (str.length > 0) {
            str += ' ';
        }
        str += `${version.sampleRate / 1000} kHz`;
    }
    if (version.bitDepth) {
        if (str.length > 0) {
            str += ', ';
        }
        str += `${version.bitDepth}-bit`;
    }

    return str;
}

export function displayAlbumVersionQualities(
    versions: Api.AlbumVersionQuality[],
    maxCharacters: number = 25,
): string {
    let str = displayAlbumVersionQuality(versions[0]!);
    let count = 1;

    for (let i = 1; i < versions.length; i++) {
        const display = displayAlbumVersionQuality(versions[i]!);

        if (str.length + display.length + ' / '.length > maxCharacters) break;

        str += ' / ' + display;
        count++;
    }

    if (versions.length - count > 0) {
        str += ` (+${versions.length - count})`;
    }

    return str;
}

export function displayApiSource(source: ApiSource) {
    switch (source) {
        case 'TIDAL':
            return 'Tidal';
        case 'QOBUZ':
            return 'Qobuz';
        case 'YT':
            return 'YouTube Music';
        case 'LIBRARY':
            return 'Library';
        default:
            source satisfies never;
            throw new Error(`Invalid ApiSource: ${source}`);
    }
}

export function downloadTaskStateClassName(state: Api.DownloadTaskState) {
    switch (state) {
        case 'PENDING':
            return 'pending';
        case 'PAUSED':
            return 'paused';
        case 'CANCELLED':
            return 'cancelled';
        case 'STARTED':
            return 'started';
        case 'ERROR':
            return 'error';
        case 'FINISHED':
            return 'finished';
        default:
            state satisfies never;
            throw new Error(`Invalid state: ${state}`);
    }
}

export function displayDownloadTaskState(state: Api.DownloadTaskState) {
    switch (state) {
        case 'PENDING':
            return 'Pending';
        case 'PAUSED':
            return 'Paused';
        case 'CANCELLED':
            return 'Cancelled';
        case 'STARTED':
            return 'Started';
        case 'ERROR':
            return 'Error';
        case 'FINISHED':
            return 'Finished';
        default:
            state satisfies never;
            throw new Error(`Invalid state: ${state}`);
    }
}
