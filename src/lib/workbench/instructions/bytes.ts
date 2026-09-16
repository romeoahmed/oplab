import type { ImageSegment } from '$lib/protocol/generated/ImageSegment';
import { formatAddress, parseAddress } from '$lib/protocol/scalars';

/**
 * Return a view of one ELF segment's file bytes, excluding zero-fill and page padding.
 *
 * @throws RangeError - The file extent is empty, nonintegral or outside the image.
 */
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

/**
 * Return a view of up to 64 KiB at a byte offset and calculate its guest address.
 *
 * @remarks
 * The offset need not be an instruction boundary; the decoder validates the bytes.
 * The returned bytes share the input buffer.
 *
 * @throws RangeError - The offset, canonical base address or full input extent is invalid.
 */
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
