import type { Locale } from '$lib/paraglide/runtime.js';

/**
 * Select the first supported language preference, falling back to English.
 *
 * @remarks
 * Invalid tags and Traditional Chinese are skipped; only Hans maps to `zh-CN`.
 */
export function preferredLocale(languages: readonly string[]): Locale {
  for (const language of languages) {
    try {
      const locale = new Intl.Locale(language).maximize();
      if (locale.language === 'zh' && locale.script === 'Hans') return 'zh-CN';
      if (locale.language === 'en') return 'en';
    } catch {
      // Ignore malformed tags and continue through the preference list.
    }
  }
  return 'en';
}
