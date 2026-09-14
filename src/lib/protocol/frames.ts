import type { Command } from './generated/Command';
import type { DesktopCall } from './generated/DesktopCall';
import type { Response } from './generated/Response';
import type { StreamEvent } from './generated/StreamEvent';

const controlLimit = 1024 * 1024;
const binaryLimit = 65536;
const encoder = new TextEncoder();
const decoder = new TextDecoder('utf-8', { fatal: true });

type Frame = { kind: number; bytes: Uint8Array };
function frame(kind: number, bytes: Uint8Array): Uint8Array {
  const limit = kind === 1 ? binaryLimit : controlLimit;
  if (bytes.length === 0 || bytes.length > limit) throw new RangeError('Invalid frame length');
  const result = new Uint8Array(8 + bytes.length);
  result.set([79, 80, kind, 0]);
  new DataView(result.buffer).setUint32(4, bytes.length, true);
  result.set(bytes, 8);
  return result;
}

function join(parts: Uint8Array[]): Uint8Array {
  const result = new Uint8Array(parts.reduce((length, part) => length + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    result.set(part, offset);
    offset += part.length;
  }
  return result;
}

class Reader {
  private offset = 0;
  constructor(private readonly input: Uint8Array) {}
  frame(): Frame {
    const start = this.offset;
    if (this.input.length - start < 8) throw new RangeError('Truncated frame');
    const kind = this.input[start + 2];
    if (
      this.input[start] !== 79 ||
      this.input[start + 1] !== 80 ||
      this.input[start + 3] !== 0 ||
      kind === undefined ||
      kind > 2
    )
      throw new RangeError('Invalid frame header');
    const length = new DataView(this.input.buffer, this.input.byteOffset + start, 8).getUint32(
      4,
      true,
    );
    if (
      length === 0 ||
      length > (kind === 1 ? binaryLimit : controlLimit) ||
      this.input.length - start - 8 < length
    )
      throw new RangeError('Invalid frame length');
    this.offset += 8 + length;
    return { kind, bytes: this.input.subarray(start + 8, this.offset) };
  }
  json(kind: number): unknown {
    const value = this.frame();
    if (value.kind !== kind) throw new RangeError('Unexpected frame kind');
    // This is the sole JSON type boundary. Rust validates these DTOs before IPC;
    // consumers additionally check exact scalars, identities, and delta coherence.
    return JSON.parse(decoder.decode(value.bytes));
  }
  payload(length: number): Uint8Array {
    if (!Number.isInteger(length) || length < 1 || length > controlLimit)
      throw new RangeError('Invalid payload length');
    const result = new Uint8Array(length);
    let offset = 0;
    while (offset < length) {
      const value = this.frame();
      if (value.kind !== 1 || value.bytes.length !== Math.min(binaryLimit, length - offset))
        throw new RangeError('Invalid payload chunk');
      result.set(value.bytes, offset);
      offset += value.bytes.length;
    }
    return result;
  }
  end(): void {
    if (this.offset !== this.input.length) throw new RangeError('Unexpected trailing frame');
  }
}

/** Encode one desktop invocation; complete ELF bytes stay binary across IPC. */
export function encodeCall(call: DesktopCall, image: Uint8Array | null): Uint8Array {
  const command: Command = call.command;
  const required = command.type === 'load' ? command.data.image_bytes : null;
  if (required !== (image?.length ?? null)) throw new RangeError('Mismatched load image');
  if (required !== null && (required < 1 || required > controlLimit))
    throw new RangeError('Invalid image length');
  const parts = [frame(0, encoder.encode(JSON.stringify(call)))];
  if (image !== null)
    for (let offset = 0; offset < image.length; offset += binaryLimit)
      parts.push(frame(1, image.subarray(offset, offset + binaryLimit)));
  return join(parts);
}

/** A response is accepted only after every declared binary file has arrived. */
export function decodeResponse(buffer: ArrayBuffer): {
  response: Response;
  payloads: Uint8Array[];
} {
  const reader = new Reader(new Uint8Array(buffer));
  const response = reader.json(0) as Response;
  const result = response.result;
  const lengths =
    result.type === 'assembled'
      ? [result.data.object_bytes, result.data.image_bytes]
      : result.type === 'observed' && result.data.memory !== null
        ? [result.data.memory.length]
        : [];
  const payloads = lengths.map((length) => reader.payload(length));
  reader.end();
  return { response, payloads };
}

/** A channel event retains its binary window and explicit subscription identity. */
export function decodeStream(buffer: ArrayBuffer): {
  event: StreamEvent;
  memory: Uint8Array | null;
} {
  const reader = new Reader(new Uint8Array(buffer));
  const event = reader.json(2) as StreamEvent;
  const update = event.update;
  const length =
    update.type === 'full'
      ? (update.data.memory?.length ?? 0)
      : update.type === 'delta'
        ? update.data.memory_bytes
        : 0;
  if (length > binaryLimit) throw new RangeError('Oversized memory observation');
  const memory = length === 0 ? null : reader.payload(length);
  reader.end();
  return { event, memory };
}
