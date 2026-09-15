import { expect, test } from 'vitest';
import fc from 'fast-check';
import { initialMemory } from '$lib/workbench/machine/memory';
import { formatAddress, parseAddress } from '$lib/protocol/scalars';
import type { ImageInfo } from '$lib/protocol/generated/ImageInfo';

const maximum = (1n << 64n) - 1n;

test('the initial viewport fits every nonempty readable segment, even at the end of the address space', () => {
  fc.assert(
    fc.property(
      fc.bigInt({ min: 0n, max: maximum }),
      fc.integer({ min: 1, max: 4096 }),
      (start, requested) => {
        const length = Number(
          BigInt(requested) < maximum - start + 1n ? BigInt(requested) : maximum - start + 1n,
        );
        const image: ImageInfo = {
          entry: formatAddress(start),
          segments: [
            {
              address: formatAddress(start),
              memory_bytes: String(length),
              file_offset: 0,
              file_bytes: 0,
              flags: 6,
            },
          ],
          symbols: [],
          symbols_truncated: false,
        };
        const window = initialMemory(image);
        if (window === null) throw new Error('Readable segment was ignored');
        expect(window.length).toBeGreaterThan(0);
        expect(parseAddress(window.address)).toBeGreaterThanOrEqual(start);
        expect(parseAddress(window.address) + BigInt(window.length)).toBeLessThanOrEqual(
          start + BigInt(length),
        );
        expect(
          initialMemory({
            ...image,
            segments: image.segments.map((segment) => ({ ...segment, flags: 1 })),
          }),
        ).toBeNull();
      },
    ),
  );
});
