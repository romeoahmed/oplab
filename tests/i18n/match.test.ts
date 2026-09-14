import { expect, test } from 'vitest';
import fc from 'fast-check';
import { preferredLocale } from '$lib/i18n/match';

test('resolves supported aliases and preserves user preference order', () => {
  expect(preferredLocale(['zh-Hans-SG', 'en-US'])).toBe('zh-CN');
  expect(preferredLocale(['en-GB', 'zh-CN'])).toBe('en');
  expect(preferredLocale(['zh-Hant', 'zh-TW'])).toBe('en');
  expect(preferredLocale(['zh-Hans-invalid-tag'])).toBe('en');
  expect(preferredLocale(['zh-HansWrong'])).toBe('en');
});

test('unsupported preferences cannot displace a later supported language', () => {
  fc.assert(
    fc.property(
      fc.array(fc.constantFrom('fr-FR', 'de', 'ja-JP', 'zh-Hant', 'zh-TW', 'zh-HK', '', 'bad_tag')),
      fc.constantFrom('zh', 'zh-CN', 'ZH-cn', 'zh-SG', 'zh-Hans-HK', 'zh-CN-u-nu-hanidec'),
      (unsupported, supported) => {
        expect(preferredLocale([...unsupported, supported, 'en'])).toBe('zh-CN');
        expect(preferredLocale([...unsupported, 'en-GB', supported])).toBe('en');
      },
    ),
  );
});
