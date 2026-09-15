import { expect, test } from 'vitest';

import english from '../../messages/en.json';
import chinese from '../../messages/zh-CN.json';

test('both catalogs provide nonempty messages with the same interpolation inputs', () => {
  expect(Object.keys(chinese).sort()).toEqual(Object.keys(english).sort());
  for (const key of Object.keys(english) as (keyof typeof english)[]) {
    for (const text of [english[key], chinese[key]]) expect(text.trim()).not.toBe('');
    const parameters = (text: string) =>
      [...text.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();
    expect(parameters(chinese[key]), key).toEqual(parameters(english[key]));
  }
});
