import { expect, test } from 'vitest';
import fc from 'fast-check';
import { defaultPreferences, readPreferences } from '$lib/workbench/preferences';

test('untrusted preferences always produce finite, usable layout and text sizes', () => {
  fc.assert(
    fc.property(
      fc.oneof(
        fc.jsonValue(),
        fc.record({
          font: fc.constantFrom('system', 'jetbrains', 'unknown'),
          fontSize: fc.double(),
          wrap: fc.boolean(),
          inspectorWidth: fc.double(),
          memoryHeight: fc.double(),
        }),
      ),
      (input) => {
        const result = readPreferences(input);
        expect(readPreferences(result)).toEqual(result);
        expect(result.fontSize).toBeGreaterThanOrEqual(12);
        expect(result.fontSize).toBeLessThanOrEqual(22);
        expect(result.inspectorWidth).toBeGreaterThanOrEqual(26);
        expect(result.inspectorWidth).toBeLessThanOrEqual(45);
        expect(result.memoryHeight).toBeGreaterThanOrEqual(20);
        expect(result.memoryHeight).toBeLessThanOrEqual(45);
        expect(Number.isInteger(result.fontSize)).toBe(true);
        expect(['jetbrains', 'system']).toContain(result.font);
      },
    ),
  );
});

test('valid preferences survive recovery and invalid fields fall back independently', () => {
  fc.assert(
    fc.property(
      fc.record({
        font: fc.constantFrom('jetbrains', 'system'),
        fontSize: fc.integer({ min: 12, max: 22 }),
        wrap: fc.boolean(),
        inspectorWidth: fc.integer({ min: 26, max: 45 }),
        memoryHeight: fc.integer({ min: 20, max: 45 }),
      }),
      (preferences) => {
        expect(readPreferences(preferences)).toEqual(preferences);
      },
    ),
  );
  expect(
    readPreferences({ font: 'system', fontSize: NaN, wrap: true, memoryHeight: Infinity }),
  ).toEqual({ ...defaultPreferences, font: 'system', wrap: true });
});
