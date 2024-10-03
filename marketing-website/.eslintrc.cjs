module.exports = {
    root: true,
    parser: '@typescript-eslint/parser',
    plugins: ['@typescript-eslint'],
    extends: [
        'eslint:recommended',
        'plugin:@typescript-eslint/recommended',
        'plugin:diff/diff',
        'prettier',
    ],
    parserOptions: {
        project: ['./tsconfig.json'],
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
        'no-unused-vars': 'off', // prefer @typescript-eslint/no-unused-vars
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
    overrides: [
        {
            files: ['**/*.ts', '**/*.tsx'],
            rules: {
                '@typescript-eslint/naming-convention': [
                    'error',
                    {
                        selector: 'objectLiteralProperty',
                        format: null,
                        custom: { regex: '.+', match: true },
                    },
                ],
            },
        },
        {
            files: ['**/sst-env.d.ts', '**/sst.config.ts'],
            rules: {
                '@typescript-eslint/triple-slash-reference': 'off',
            },
        },
    ],
};
