import type { Config } from 'stylelint';

export default {
  extends: ['stylelint-config-standard'],
  overrides: [
    {
      files: ['src/lib/workbench/editor/editor.css'],
      // CodeMirror owns its camelCase class names; application classes stay kebab-case.
      rules: { 'selector-class-pattern': '^(?:cm-[A-Za-z0-9-]+|[a-z][a-z0-9]*(?:-[a-z0-9]+)*)$' },
    },
  ],
} satisfies Config;
