import type { ImageInfo } from '$lib/protocol/generated/ImageInfo';
import type { MemoryWindow } from '$lib/protocol/generated/MemoryWindow';
import { parseAddress, parseCounter } from '$lib/protocol/scalars';

/** Maximum bytes in one desktop memory patch. */
export const patchLimit = 4096;

/**
 * Decode hexadecimal byte pairs with optional whitespace between pairs.
 *
 * @remarks
 * Input is capped at 12,288 UTF-16 code units; decoded output is capped at 4 KiB.
 *
 * @throws RangeError - Input is empty, malformed or exceeds either limit.
 */
export function patchBytes(text: string): Uint8Array {
  if (text.length > patchLimit * 3 || !/^(?:[\da-f]{2}\s*)+$/i.test(text.trim()))
    throw new RangeError('Invalid patch bytes');
  const hex = text.replace(/\s/g, '');
  if (hex.length > patchLimit * 2) throw new RangeError('Invalid patch length');
  return Uint8Array.fromHex(hex);
}

/**
 * Select up to 64 bytes at a readable segment's start for initial inspection.
 *
 * @remarks
 * Prefer writable data, then the entry segment, then the first readable segment.
 *
 * @returns A segment-bounded window, or `null` when no readable storage exists.
 * @throws RangeError - An inspected ELF address or size has invalid wire encoding.
 */
export function initialMemory(image: ImageInfo): MemoryWindow | null {
  const readable = image.segments.filter(
    (segment) => (segment.flags & 4) !== 0 && parseCounter(segment.memory_bytes) > 0n,
  );
  const entry = parseAddress(image.entry);
  const segment =
    readable.find((segment) => (segment.flags & 2) !== 0) ??
    readable.find((segment) => {
      const start = parseAddress(segment.address);
      return entry >= start && entry - start < parseCounter(segment.memory_bytes);
    }) ??
    readable[0];
  if (segment === undefined) return null;
  const length = parseCounter(segment.memory_bytes);
  return { address: segment.address, length: Number(length < 64n ? length : 64n) };
}
