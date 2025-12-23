import { describe, it, expect, vi } from 'vitest';
import {
    getLines,
    getMessages,
    getBytes,
    EventSourceMessage,
} from '../../src/vendored/fetch-event-source';

describe('getLines', () => {
    it('parses complete lines ending with LF', () => {
        const lines: Array<{ line: string; fieldLength: number }> = [];
        const onChunk = getLines((line, fieldLength) => {
            lines.push({ line: new TextDecoder().decode(line), fieldLength });
        });

        const encoder = new TextEncoder();
        onChunk(encoder.encode('data: hello\n'));

        expect(lines).toHaveLength(1);
        expect(lines[0].line).toBe('data: hello');
        expect(lines[0].fieldLength).toBe(4); // "data".length
    });

    it('parses complete lines ending with CR', () => {
        const lines: string[] = [];
        const onChunk = getLines((line) => {
            lines.push(new TextDecoder().decode(line));
        });

        const encoder = new TextEncoder();
        onChunk(encoder.encode('data: hello\r'));

        expect(lines).toEqual(['data: hello']);
    });

    it('handles CRLF line endings', () => {
        const lines: string[] = [];
        const onChunk = getLines((line) => {
            lines.push(new TextDecoder().decode(line));
        });

        const encoder = new TextEncoder();
        onChunk(encoder.encode('data: hello\r\n'));

        expect(lines).toEqual(['data: hello']);
    });

    it('handles partial chunks across multiple calls', () => {
        const lines: string[] = [];
        const onChunk = getLines((line) => {
            lines.push(new TextDecoder().decode(line));
        });

        const encoder = new TextEncoder();
        onChunk(encoder.encode('data: hel'));
        onChunk(encoder.encode('lo world\n'));

        expect(lines).toEqual(['data: hello world']);
    });

    it('handles multiple lines in a single chunk', () => {
        const lines: string[] = [];
        const onChunk = getLines((line) => {
            lines.push(new TextDecoder().decode(line));
        });

        const encoder = new TextEncoder();
        onChunk(encoder.encode('data: line1\ndata: line2\n'));

        expect(lines).toEqual(['data: line1', 'data: line2']);
    });

    it('handles empty lines (message boundaries)', () => {
        const lines: string[] = [];
        const onChunk = getLines((line) => {
            lines.push(new TextDecoder().decode(line));
        });

        const encoder = new TextEncoder();
        onChunk(encoder.encode('data: hello\n\n'));

        expect(lines).toEqual(['data: hello', '']);
    });

    it('correctly identifies field length for colon position', () => {
        const results: Array<{ line: string; fieldLength: number }> = [];
        const onChunk = getLines((line, fieldLength) => {
            results.push({
                line: new TextDecoder().decode(line),
                fieldLength,
            });
        });

        const encoder = new TextEncoder();
        onChunk(encoder.encode('event: custom\n'));
        onChunk(encoder.encode('id: 123\n'));
        onChunk(encoder.encode('retry: 5000\n'));

        expect(results[0].fieldLength).toBe(5); // "event".length
        expect(results[1].fieldLength).toBe(2); // "id".length
        expect(results[2].fieldLength).toBe(5); // "retry".length
    });

    it('handles lines without colon (comments or field-only)', () => {
        const results: Array<{ line: string; fieldLength: number }> = [];
        const onChunk = getLines((line, fieldLength) => {
            results.push({
                line: new TextDecoder().decode(line),
                fieldLength,
            });
        });

        const encoder = new TextEncoder();
        onChunk(encoder.encode(': this is a comment\n'));

        // Comment lines have colon at position 0, so fieldLength is 0
        expect(results[0].fieldLength).toBe(0);
    });
});

describe('getMessages', () => {
    it('parses data field', () => {
        const messages: EventSourceMessage[] = [];
        const onLine = getMessages(
            () => {},
            () => {},
            (msg) => messages.push(msg),
        );

        const encoder = new TextEncoder();
        onLine(encoder.encode('data: hello'), 4);
        onLine(new Uint8Array(0), -1); // empty line = end of message

        expect(messages).toHaveLength(1);
        expect(messages[0].data).toBe('hello');
    });

    it('parses data field with space after colon', () => {
        const messages: EventSourceMessage[] = [];
        const onLine = getMessages(
            () => {},
            () => {},
            (msg) => messages.push(msg),
        );

        const encoder = new TextEncoder();
        // "data: hello" - space after colon should be skipped
        onLine(encoder.encode('data: hello'), 4);
        onLine(new Uint8Array(0), -1);

        expect(messages[0].data).toBe('hello');
    });

    it('accumulates multi-line data', () => {
        const messages: EventSourceMessage[] = [];
        const onLine = getMessages(
            () => {},
            () => {},
            (msg) => messages.push(msg),
        );

        const encoder = new TextEncoder();
        onLine(encoder.encode('data: line1'), 4);
        onLine(encoder.encode('data: line2'), 4);
        onLine(new Uint8Array(0), -1);

        expect(messages[0].data).toBe('line1\nline2');
    });

    it('parses event field', () => {
        const messages: EventSourceMessage[] = [];
        const onLine = getMessages(
            () => {},
            () => {},
            (msg) => messages.push(msg),
        );

        const encoder = new TextEncoder();
        onLine(encoder.encode('event: custom'), 5);
        onLine(encoder.encode('data: payload'), 4);
        onLine(new Uint8Array(0), -1);

        expect(messages[0].event).toBe('custom');
        expect(messages[0].data).toBe('payload');
    });

    it('invokes onId callback when id field received', () => {
        const onId = vi.fn();
        const messages: EventSourceMessage[] = [];
        const onLine = getMessages(
            onId,
            () => {},
            (msg) => messages.push(msg),
        );

        const encoder = new TextEncoder();
        onLine(encoder.encode('id: abc-123'), 2);
        onLine(encoder.encode('data: test'), 4);
        onLine(new Uint8Array(0), -1);

        expect(onId).toHaveBeenCalledWith('abc-123');
        expect(messages[0].id).toBe('abc-123');
    });

    it('invokes onRetry callback when retry field received', () => {
        const onRetry = vi.fn();
        const messages: EventSourceMessage[] = [];
        const onLine = getMessages(
            () => {},
            onRetry,
            (msg) => messages.push(msg),
        );

        const encoder = new TextEncoder();
        onLine(encoder.encode('retry: 5000'), 5);
        onLine(encoder.encode('data: test'), 4);
        onLine(new Uint8Array(0), -1);

        expect(onRetry).toHaveBeenCalledWith(5000);
        expect(messages[0].retry).toBe(5000);
    });

    it('ignores invalid retry values (non-integer)', () => {
        const onRetry = vi.fn();
        const onLine = getMessages(
            () => {},
            onRetry,
            () => {},
        );

        const encoder = new TextEncoder();
        onLine(encoder.encode('retry: invalid'), 5);
        onLine(new Uint8Array(0), -1);

        expect(onRetry).not.toHaveBeenCalled();
    });

    it('creates new message after empty line', () => {
        const messages: EventSourceMessage[] = [];
        const onLine = getMessages(
            () => {},
            () => {},
            (msg) => messages.push(msg),
        );

        const encoder = new TextEncoder();
        onLine(encoder.encode('data: first'), 4);
        onLine(new Uint8Array(0), -1);
        onLine(encoder.encode('data: second'), 4);
        onLine(new Uint8Array(0), -1);

        expect(messages).toHaveLength(2);
        expect(messages[0].data).toBe('first');
        expect(messages[1].data).toBe('second');
    });

    it('ignores comment lines (fieldLength 0)', () => {
        const messages: EventSourceMessage[] = [];
        const onLine = getMessages(
            () => {},
            () => {},
            (msg) => messages.push(msg),
        );

        const encoder = new TextEncoder();
        // Comments have colon at position 0
        onLine(encoder.encode(': this is a comment'), 0);
        onLine(encoder.encode('data: hello'), 4);
        onLine(new Uint8Array(0), -1);

        expect(messages).toHaveLength(1);
        expect(messages[0].data).toBe('hello');
    });

    it('initializes message with empty strings', () => {
        const messages: EventSourceMessage[] = [];
        const onLine = getMessages(
            () => {},
            () => {},
            (msg) => messages.push(msg),
        );

        // Empty message (just send empty line)
        onLine(new Uint8Array(0), -1);

        expect(messages).toHaveLength(1);
        expect(messages[0].data).toBe('');
        expect(messages[0].event).toBe('');
        expect(messages[0].id).toBe('');
        expect(messages[0].retry).toBeUndefined();
    });
});

describe('getBytes', () => {
    it('converts ReadableStream to callback pattern', async () => {
        const chunks: Uint8Array[] = [];
        const encoder = new TextEncoder();

        const stream = new ReadableStream<Uint8Array>({
            start(controller) {
                controller.enqueue(encoder.encode('chunk1'));
                controller.enqueue(encoder.encode('chunk2'));
                controller.close();
            },
        });

        await getBytes(stream, (chunk) => {
            chunks.push(chunk);
        });

        expect(chunks).toHaveLength(2);
        expect(new TextDecoder().decode(chunks[0])).toBe('chunk1');
        expect(new TextDecoder().decode(chunks[1])).toBe('chunk2');
    });

    it('resolves when stream closes', async () => {
        const stream = new ReadableStream<Uint8Array>({
            start(controller) {
                controller.close();
            },
        });

        await expect(getBytes(stream, () => {})).resolves.toBeUndefined();
    });
});

describe('SSE message format integration', () => {
    it('parses a complete SSE message with all fields', () => {
        const messages: EventSourceMessage[] = [];
        const onId = vi.fn();
        const onRetry = vi.fn();

        const onChunk = getLines(
            getMessages(onId, onRetry, (msg) => messages.push(msg)),
        );

        const encoder = new TextEncoder();
        onChunk(
            encoder.encode(
                'id: msg-001\nevent: update\nretry: 3000\ndata: {"status":"ok"}\n\n',
            ),
        );

        expect(messages).toHaveLength(1);
        expect(messages[0]).toEqual({
            id: 'msg-001',
            event: 'update',
            data: '{"status":"ok"}',
            retry: 3000,
        });
        expect(onId).toHaveBeenCalledWith('msg-001');
        expect(onRetry).toHaveBeenCalledWith(3000);
    });

    it('handles typical HyperChad view message', () => {
        const messages: EventSourceMessage[] = [];

        const onChunk = getLines(
            getMessages(
                () => {},
                () => {},
                (msg) => messages.push(msg),
            ),
        );

        const encoder = new TextEncoder();
        onChunk(
            encoder.encode('event: view\ndata: <div>Hello World</div>\n\n'),
        );

        expect(messages).toHaveLength(1);
        expect(messages[0].event).toBe('view');
        expect(messages[0].data).toBe('<div>Hello World</div>');
    });

    it('handles partial_view message with ID', () => {
        const messages: EventSourceMessage[] = [];

        const onChunk = getLines(
            getMessages(
                () => {},
                () => {},
                (msg) => messages.push(msg),
            ),
        );

        const encoder = new TextEncoder();
        onChunk(
            encoder.encode(
                'event: partial_view\nid: element-123\ndata: <div>Updated</div>\n\n',
            ),
        );

        expect(messages).toHaveLength(1);
        expect(messages[0].event).toBe('partial_view');
        expect(messages[0].id).toBe('element-123');
        expect(messages[0].data).toBe('<div>Updated</div>');
    });
});
