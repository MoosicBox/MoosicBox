import { format, parseISO } from 'date-fns';

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

export function formatClassName(value: string): string {
    return value
        .replace(/ /g, '-')
        .replace(/[^\w-]/g, '_')
        .toLocaleLowerCase();
}

export function getSize(size: number) {
    if (size < 1024) {
        return `${size}B`;
    } else if (size / 1024 < 1024) {
        return `${(size / 1024).toFixed(1)}KiB`;
    } else if (size / 1024 / 1024 < 1024) {
        return `${(size / 1024 / 1024).toFixed(1)}MiB`;
    } else {
        return `${(size / 1024 / 1024 / 1024).toFixed(1)}GiB`;
    }
}
