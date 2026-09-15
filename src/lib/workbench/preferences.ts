export type Preferences = {
  font: 'jetbrains' | 'system';
  /** Editor font size in CSS pixels. */
  fontSize: number;
  wrap: boolean;
  /** Inspector width as a percentage of the workbench width. */
  inspectorWidth: number;
  /** Observation panel height as a percentage of the workbench height. */
  memoryHeight: number;
};
export const defaultPreferences: Preferences = {
  font: 'jetbrains',
  fontSize: 14,
  wrap: false,
  inspectorWidth: 30,
  memoryHeight: 28,
};

/**
 * Recover preferences from parsed JSON, validating each field independently.
 * Finite sizes are clamped and rounded; missing or invalid values use defaults.
 */
export function readPreferences(value: unknown): Preferences {
  const record = typeof value === 'object' && value !== null ? value : {};
  const bounded = (key: string, minimum: number, maximum: number, fallback: number) => {
    const item: unknown = Reflect.get(record, key);
    return typeof item === 'number' && Number.isFinite(item)
      ? Math.round(Math.max(minimum, Math.min(maximum, item)))
      : fallback;
  };
  return {
    font: Reflect.get(record, 'font') === 'system' ? 'system' : 'jetbrains',
    fontSize: bounded('fontSize', 12, 22, defaultPreferences.fontSize),
    wrap: Reflect.get(record, 'wrap') === true,
    inspectorWidth: bounded('inspectorWidth', 26, 45, defaultPreferences.inspectorWidth),
    memoryHeight: bounded('memoryHeight', 20, 45, defaultPreferences.memoryHeight),
  };
}
