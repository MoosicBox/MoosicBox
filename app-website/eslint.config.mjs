import typescriptEslint from '@typescript-eslint/eslint-plugin';
import tsParser from '@typescript-eslint/parser';
import parser from 'astro-eslint-parser';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import js from '@eslint/js';
import { FlatCompat } from '@eslint/eslintrc';
import globals from 'globals';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const compat = new FlatCompat({
    baseDirectory: __dirname,
    recommendedConfig: js.configs.recommended,
    allConfig: js.configs.all,
});

const tsFiles = ['**/*.ts', '**/*.tsx'];

const jsFiles = ['**/*.js', '**/*.jsx', '**/*.cjs', '**/*.mjs'];
// ...tsEslint.configs.recommendedTypeChecked.map((config) => ({
//     ...config,
//     files: ['**/*.ts'], // We use TS config only for TS files
//   })),

export default [
    {
        ignores: ['.sst'],
    },
    ...compat.extends(
        'eslint:recommended',
        'plugin:@typescript-eslint/recommended',
    ),
    {
        plugins: {
            '@typescript-eslint': typescriptEslint,
        },
        languageOptions: {
            parser: tsParser,
            ecmaVersion: 5,
            sourceType: 'script',

            parserOptions: {
                project: ['./tsconfig.json'],
            },
        },
        rules: {
            'import/prefer-default-export': 'off',
            '@typescript-eslint/ban-ts-comment': 'off',
            '@typescript-eslint/naming-convention': [
                'error',
                {
                    selector: 'default',
                    format: ['camelCase'],
                    leadingUnderscore: 'allow',
                    trailingUnderscore: 'forbid',
                },
                {
                    selector: 'variable',
                    format: ['camelCase', 'PascalCase', 'UPPER_CASE'],
                    leadingUnderscore: 'allow',
                    trailingUnderscore: 'forbid',
                },
                {
                    selector: 'typeLike',
                    format: ['PascalCase'],
                },
            ],
            'no-unused-vars': 'off',
            '@typescript-eslint/no-unused-vars': [
                'error',
                {
                    argsIgnorePattern: '^_',
                    varsIgnorePattern: '^_',
                    caughtErrorsIgnorePattern: '^_',
                },
            ],
            '@typescript-eslint/no-namespace': 'off',
        },
        files: [...tsFiles, '**/*.astro'],
    },
    {
        files: [...jsFiles, ...tsFiles],
        languageOptions: {
            globals: {
                ...globals.node,
                ...globals.browser,
            },
        },
        rules: {
            '@typescript-eslint/naming-convention': [
                'error',
                {
                    selector: 'objectLiteralProperty',
                    format: null,

                    custom: {
                        regex: '.+',
                        match: true,
                    },
                },
            ],
        },
    },
    {
        files: ['**/*env.d.ts'],
        linterOptions: {
            reportUnusedDisableDirectives: 'off',
        },
    },
    {
        files: ['**/*env.d.ts', '**/sst.config.ts', '**/infra/*.ts'],
        rules: {
            '@typescript-eslint/triple-slash-reference': 'off',
        },
    },
    {
        files: ['**/*.astro'],
        languageOptions: {
            parser: parser,
            ecmaVersion: 5,
            sourceType: 'script',

            parserOptions: {
                parser: '@typescript-eslint/parser',
                extraFileExtensions: ['.astro'],
            },
        },
        rules: {
            '@typescript-eslint/naming-convention': 'off',
        },
    },
];
