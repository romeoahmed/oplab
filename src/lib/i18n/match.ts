import type { Locale } from '$lib/paraglide/runtime.js';

/** Resolve supported language tags without converting Traditional Chinese to Simplified. */
export function preferredLocale(languages: readonly string[]): Locale {
  for (const language of languages) {
    try {
      const locale = new Intl.Locale(language).maximize();
      if (locale.language === 'zh' && locale.script === 'Hans') return 'zh-CN';
      if (locale.language === 'en') return 'en';
    } catch {
      // Invalid stored tags must not prevent trying the next user preference.
    }
  }
  return 'en';
}
