import type { ImageSegment } from '$lib/protocol/generated/ImageSegment';
import { formatAddress, parseAddress } from '$lib/protocol/scalars';

/** Select one file-backed ELF segment without inventing bytes for BSS or gaps. */
export function segmentBytes(image: Uint8Array, segment: ImageSegment): Uint8Array {
  const start = segment.file_offset;
  const length = segment.file_bytes;
  if (
    !Number.isSafeInteger(start) ||
    !Number.isSafeInteger(length) ||
    start < 0 ||
    length < 1 ||
    start > image.length - length
  )
    throw new RangeError('Invalid file-backed segment');
  return image.subarray(start, start + length);
}

/** Slice at an explicit byte offset; decoding must still establish instruction validity. */
export function decodeWindow(bytes: Uint8Array, base: string, offset: number) {
  if (!Number.isSafeInteger(offset) || offset < 0 || offset >= bytes.length)
    throw new RangeError('Invalid byte window');
  const address = parseAddress(base);
  if (address + BigInt(bytes.length) > 1n << 64n) throw new RangeError('Address overflow');
  return {
    bytes: bytes.subarray(offset, offset + 65536),
    base: formatAddress(address + BigInt(offset)),
  };
}
