import type { ImageInfo } from '$lib/protocol/generated/ImageInfo';
import type { MemoryWindow } from '$lib/protocol/generated/MemoryWindow';
import { parseAddress, parseCounter } from '$lib/protocol/scalars';

/**
 * Select up to 64 bytes at a readable segment's start for initial inspection.
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
