#!/usr/bin/env node

/**
 * Test script to verify all templates render correctly with the new partial system
 */

const fs = require('fs');
const path = require('path');
const matter = require('gray-matter');

const templatesDir = path.join(__dirname, 'templates');
const partialsDir = path.join(templatesDir, 'partials');

// Mock environment for testing
process.env.GITHUB_CONTEXT = JSON.stringify({
    repository: 'test-owner/test-repo',
    run_id: '12345',
    event_name: 'issue_comment',
    event: {
        issue: {
            number: 42,
            title: 'Test Issue',
            body: 'Test issue body',
        },
        comment: {
            body: 'Test comment',
        },
        pull_request: {
            number: 99,
            head: { ref: 'feature-branch' },
        },
    },
});

// Create a temporary output file
const tempOutput = '/tmp/github_output_test';
fs.writeFileSync(tempOutput, '');
process.env.GITHUB_OUTPUT = tempOutput;

/**
 * Load and render a partial template
 */
function loadPartial(name, parentVars, overrideParams = {}) {
    const partialPath = path.join(partialsDir, `${name}.md`);

    if (!fs.existsSync(partialPath)) {
        throw new Error(
            `Partial not found: ${name} (looked at ${partialPath})`,
        );
    }

    const content = fs.readFileSync(partialPath, 'utf8');
    const { data: partialFrontmatter, content: partialBody } = matter(content);

    const mergedVars = {
        ...partialFrontmatter,
        ...parentVars,
        ...overrideParams,
    };

    return renderTemplate(partialBody, mergedVars);
}

/**
 * Extract balanced expression from template starting at position
 * Handles nested braces properly
 */
function extractExpression(template, startPos) {
    // Start at depth 1 because we're already inside ${ ... }
    let depth = 1;
    let inString = false;
    let stringChar = '';
    let i = startPos;

    while (i < template.length) {
        const char = template[i];
        const prevChar = i > 0 ? template[i - 1] : '';

        // Handle string boundaries (skip escaped quotes)
        if ((char === '"' || char === "'") && prevChar !== '\\') {
            if (!inString) {
                inString = true;
                stringChar = char;
            } else if (char === stringChar) {
                inString = false;
            }
        }

        // Only count braces outside of strings
        if (!inString) {
            if (char === '{') {
                depth++;
            } else if (char === '}') {
                depth--;
                if (depth === 0) {
                    return {
                        expression: template.slice(startPos, i),
                        endPos: i,
                    };
                }
            }
        }

        i++;
    }

    return null; // Unbalanced braces
}

/**
 * Evaluate a single expression
 */
function evaluateExpression(expression, vars) {
    try {
        if (/^[a-zA-Z_][a-zA-Z0-9_]*$/.test(expression.trim())) {
            const value = vars[expression.trim()];
            return value !== undefined && value !== null ? String(value) : '';
        }

        const evalContext = { ...vars };

        evalContext.derive_package_name = (packagePath) => {
            if (!packagePath || packagePath === '.') return '';
            return packagePath.split('/').pop() || '';
        };

        evalContext.include = (partialName, overrideParams = {}) => {
            return loadPartial(partialName, vars, overrideParams);
        };

        const evalFn = new Function(
            ...Object.keys(evalContext),
            `return ${expression};`,
        );
        const result = evalFn(...Object.values(evalContext));

        return result !== undefined && result !== null ? String(result) : '';
    } catch (error) {
        console.warn(
            `  ⚠️ Expression failed: ${expression} - ${error.message}`,
        );
        return '${' + expression + '}';
    }
}

/**
 * Render template by replacing ${var_name} with values
 */
function renderTemplate(template, vars) {
    let result = '';
    let lastEnd = 0;
    let i = 0;

    while (i < template.length) {
        // Look for ${
        if (template[i] === '$' && template[i + 1] === '{') {
            // Add text before this expression
            result += template.slice(lastEnd, i);

            // Extract the full expression with balanced braces
            const extracted = extractExpression(template, i + 2);
            if (!extracted) {
                // Unbalanced braces, treat as literal
                result += '${';
                lastEnd = i + 2;
                i += 2;
                continue;
            }

            const { expression, endPos } = extracted;
            const replacement = evaluateExpression(expression, vars);
            result += replacement;

            lastEnd = endPos + 1;
            i = endPos + 1;
        } else {
            i++;
        }
    }

    // Add remaining text
    result += template.slice(lastEnd);
    return result;
}

/**
 * Test a single template
 */
function testTemplate(templateName) {
    const templatePath = path.join(templatesDir, `${templateName}.md`);

    if (!fs.existsSync(templatePath)) {
        console.log(`  ❌ Template file not found: ${templatePath}`);
        return false;
    }

    try {
        const content = fs.readFileSync(templatePath, 'utf8');
        const { data: frontmatter, content: templateBody } = matter(content);

        // Build test variables
        const testVars = {
            repository: 'test-owner/test-repo',
            repository_name: 'test-repo',
            run_id: '12345',
            package_path: 'packages/test',
            package_name: 'test_package',
            issue_number: '42',
            pr_number: '99',
            branch_name: 'test-branch',
            event_name: 'issue_comment',
            readme_path: 'README.md',
            ...frontmatter,
        };

        // Render frontmatter variables first
        for (const [key, value] of Object.entries(testVars)) {
            if (typeof value === 'string' && value.includes('${')) {
                testVars[key] = renderTemplate(value, testVars);
            }
        }

        // Render the template
        const rendered = renderTemplate(templateBody, testVars);

        // Check for unresolved variables or failed includes
        const unresolvedVars = rendered.match(/\$\{[^}]+\}/g) || [];
        const failedIncludes = unresolvedVars.filter((v) =>
            v.includes('include('),
        );

        if (failedIncludes.length > 0) {
            console.log(`  ❌ Failed includes: ${failedIncludes.join(', ')}`);
            return false;
        }

        // Check for partial rendering (some unresolved vars are OK if they're conditional)
        const criticalUnresolved = unresolvedVars.filter(
            (v) => !v.includes('?') && !v.includes('custom_guidelines'),
        );

        if (criticalUnresolved.length > 0) {
            console.log(
                `  ⚠️ Unresolved variables (may be OK): ${criticalUnresolved.slice(0, 3).join(', ')}${criticalUnresolved.length > 3 ? '...' : ''}`,
            );
        }

        console.log(`  ✅ Rendered successfully (${rendered.length} chars)`);
        return true;
    } catch (error) {
        console.log(`  ❌ Error: ${error.message}`);
        return false;
    }
}

/**
 * Test all partials load correctly
 */
function testPartials() {
    console.log('\n📦 Testing Partials...\n');

    const partialFiles = [];

    function findPartials(dir, prefix = '') {
        const items = fs.readdirSync(dir);
        for (const item of items) {
            const fullPath = path.join(dir, item);
            const relativePath = prefix ? `${prefix}/${item}` : item;
            if (fs.statSync(fullPath).isDirectory()) {
                findPartials(fullPath, relativePath);
            } else if (item.endsWith('.md')) {
                partialFiles.push(relativePath.replace('.md', ''));
            }
        }
    }

    findPartials(partialsDir);

    let passed = 0;
    let failed = 0;

    for (const partial of partialFiles) {
        process.stdout.write(`  ${partial}: `);
        try {
            const result = loadPartial(partial, {
                package_name: 'test_package',
                repository: 'test/repo',
                file_path: 'README.md',
            });
            if (result && result.length > 0) {
                console.log(`✅ (${result.length} chars)`);
                passed++;
            } else {
                console.log('❌ Empty result');
                failed++;
            }
        } catch (error) {
            console.log(`❌ ${error.message}`);
            failed++;
        }
    }

    return { passed, failed };
}

/**
 * Main test runner
 */
function main() {
    console.log('🧪 Testing Claude Checker Templates\n');
    console.log('='.repeat(50));

    // Test partials first
    const partialResults = testPartials();

    // Test all templates
    console.log('\n📄 Testing Templates...\n');

    const templates = [
        'code-review',
        'examples',
        'issue',
        'pr',
        'readme',
        'rustdoc',
        'security',
        'unit-tests',
    ];
    let passed = 0;
    let failed = 0;

    for (const template of templates) {
        console.log(`\n${template}:`);
        if (testTemplate(template)) {
            passed++;
        } else {
            failed++;
        }
    }

    // Summary
    console.log('\n' + '='.repeat(50));
    console.log('\n📊 Summary:\n');
    console.log(
        `  Partials: ${partialResults.passed} passed, ${partialResults.failed} failed`,
    );
    console.log(`  Templates: ${passed} passed, ${failed} failed`);

    if (failed > 0 || partialResults.failed > 0) {
        console.log('\n❌ Some tests failed!');
        process.exit(1);
    } else {
        console.log('\n✅ All tests passed!');
    }
}

main();
