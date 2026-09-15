import { localStorageKey, setLocale, type Locale } from '$lib/paraglide/runtime.js';

import { preferredLocale } from './match';

/**
 * Own locale selection and document metadata for the mounted application.
 * Changes persist in order without reloading; failed writes leave the locale intact.
 */
export function createLocaleController() {
  let current = $state<Locale>('en');
  let failed = $state(false);
  let pending = Promise.resolve();

  function synchronize(locale: Locale) {
    current = locale;
    document.documentElement.lang = locale;
    document.documentElement.dir = 'ltr';
    document.title = 'Oplab';
  }

  return {
    get current() {
      return current;
    },
    get failed() {
      return failed;
    },
    initialize() {
      let locale = preferredLocale(navigator.languages);
      try {
        const stored = localStorage.getItem(localStorageKey);
        if (stored === 'en' || stored === 'zh-CN') locale = stored;
      } catch {
        // Use the system preference when storage is unavailable.
      }
      synchronize(locale);
    },
    change(locale: Locale): Promise<void> {
      pending = pending.then(async () => {
        try {
          await setLocale(locale, { reload: false });
          synchronize(locale);
          failed = false;
        } catch {
          failed = true;
        }
      });
      return pending;
    },
  };
}
