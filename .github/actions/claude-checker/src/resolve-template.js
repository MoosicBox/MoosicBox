#!/usr/bin/env node

const fs = require('fs');
const path = require('path');
const yaml = require('js-yaml');
const matter = require('gray-matter');

/**
 * Main template resolver
 * Priority: user template_vars > frontmatter defaults > auto-detected GitHub context
 */
async function main() {
    try {
        // Step 1: Load template content
        const templateContent = loadTemplate();

        // Step 2: Parse frontmatter and template body
        const { data: frontmatter, content: templateBody } =
            matter(templateContent);

        // Step 3: Build variable resolution hierarchy
        const githubContext = JSON.parse(process.env.GITHUB_CONTEXT || '{}');
        const autoDetectedVars = buildAutoDetectedVars(githubContext);
        const userVars = parseUserVars(process.env.TEMPLATE_VARS || '{}');

        // Step 4: Merge variables (user > frontmatter > auto-detected)
        let resolvedVars = { ...autoDetectedVars };

        // Store original frontmatter template strings for second pass (before first render)
        // This preserves the raw templates so we can re-render them after user vars are applied
        const frontmatterTemplates = {};
        for (const [key, value] of Object.entries(frontmatter)) {
            if (typeof value === 'string' && value.includes('${')) {
                frontmatterTemplates[key] = value;
            }
        }

        // First pass: Apply frontmatter defaults (with interpolation support)
        // This allows frontmatter values to reference each other and use helpers
        resolvedVars = applyFrontmatterDefaults(frontmatter, resolvedVars);

        // Apply user overrides
        resolvedVars = { ...resolvedVars, ...userVars };

        // Second pass: Re-render frontmatter template strings with complete variable set
        // This ensures variables like commit_message get properly interpolated
        // after user vars (package_name, package_path, etc.) are available
        for (const [key, template] of Object.entries(frontmatterTemplates)) {
            // Only re-render if this key wasn't overridden by user vars
            if (!userVars.hasOwnProperty(key)) {
                resolvedVars[key] = renderTemplate(template, resolvedVars);
            }
        }

        // Step 5: Render template with resolved variables
        const renderedPrompt = renderTemplate(templateBody, resolvedVars);

        // Step 6: Output results
        outputResults(renderedPrompt, resolvedVars);
    } catch (error) {
        console.error('❌ Template resolution failed:', error.message);
        console.error(error.stack);
        process.exit(1);
    }
}

/**
 * Load template content from one of three sources
 */
function loadTemplate() {
    const builtInTemplate = process.env.PROMPT_TEMPLATE;
    const templateFile = process.env.PROMPT_TEMPLATE_FILE;
    const templateText = process.env.PROMPT_TEMPLATE_TEXT;

    if (templateText) {
        console.log('📝 Using inline template text');
        return templateText;
    }

    if (templateFile) {
        console.log(`📄 Loading template from file: ${templateFile}`);
        const filePath = path.resolve(
            process.env.GITHUB_WORKSPACE || process.cwd(),
            templateFile,
        );
        if (!fs.existsSync(filePath)) {
            throw new Error(`Template file not found: ${filePath}`);
        }
        return fs.readFileSync(filePath, 'utf8');
    }

    if (builtInTemplate) {
        console.log(`📦 Loading built-in template: ${builtInTemplate}`);
        const actionPath = path.resolve(__dirname, '..');
        const templatePath = path.join(
            actionPath,
            'templates',
            `${builtInTemplate}.md`,
        );
        if (!fs.existsSync(templatePath)) {
            throw new Error(`Built-in template not found: ${builtInTemplate}`);
        }
        return fs.readFileSync(templatePath, 'utf8');
    }

    throw new Error('No template source provided');
}

/**
 * Parse user-provided template vars (YAML or JSON)
 * Filters out null values to prevent them from appearing as literal "null" in templates
 */
function parseUserVars(input) {
    if (!input || input === '{}') {
        return {};
    }

    let vars;
    try {
        // Try JSON first
        vars = JSON.parse(input);
    } catch {
        // Try YAML
        try {
            vars = yaml.load(input) || {};
        } catch (error) {
            throw new Error(`Failed to parse template_vars: ${error.message}`);
        }
    }

    // Filter out null values - they should be treated as not provided
    const filtered = {};
    for (const [key, value] of Object.entries(vars)) {
        if (value !== null) {
            filtered[key] = value;
        }
    }
    return filtered;
}

/**
 * Build auto-detected variables from GitHub context
 */
function buildAutoDetectedVars(github) {
    const vars = {
        // Repository
        repository: github.repository || '',
        repository_owner: (github.repository || '').split('/')[0] || '',
        repository_name: (github.repository || '').split('/')[1] || '',

        // Workflow
        run_id: github.run_id || '',
        run_number: github.run_number || '',
        workflow: github.workflow || '',
        sha: github.sha || '',
        ref: github.ref || '',
        ref_name: github.ref_name || '',

        // Actor
        actor: github.actor || '',

        // Event type
        event_name: github.event_name || '',
    };

    // Event-specific variables
    if (github.event) {
        const event = github.event;

        // Issue-related
        if (event.issue) {
            vars.github_event_issue_number = event.issue.number || '';
            vars.github_event_issue_title = event.issue.title || '';
            vars.github_event_issue_body = event.issue.body || '';
            vars.github_event_issue_user_login = event.issue.user?.login || '';
            vars.github_event_issue_user_id = event.issue.user?.id || '';
        }

        // Comment-related
        if (event.comment) {
            vars.github_event_comment_body = event.comment.body || '';
            vars.github_event_comment_user_login =
                event.comment.user?.login || '';
            vars.github_event_comment_user_id = event.comment.user?.id || '';
            vars.github_event_comment_id = event.comment.id || '';
            vars.github_event_comment_html_url = event.comment.html_url || '';

            // PR review comment specifics
            if (event.comment.path) {
                vars.github_event_comment_path = event.comment.path;
                vars.github_event_comment_line =
                    event.comment.line || event.comment.original_line || '';
                vars.github_event_comment_side = event.comment.side || '';
                vars.github_event_comment_diff_hunk =
                    event.comment.diff_hunk || '';
            }
        }

        // PR-related
        if (event.pull_request) {
            vars.github_event_pull_request_number =
                event.pull_request.number || '';
            vars.github_event_pull_request_title =
                event.pull_request.title || '';
            vars.github_event_pull_request_body = event.pull_request.body || '';
            vars.github_event_pull_request_head_ref =
                event.pull_request.head?.ref || '';
            vars.github_event_pull_request_head_sha =
                event.pull_request.head?.sha || '';
            vars.github_event_pull_request_user_login =
                event.pull_request.user?.login || '';
        }

        // Review-related
        if (event.review) {
            vars.github_event_review_body = event.review.body || '';
            vars.github_event_review_user_login =
                event.review.user?.login || '';
            vars.github_event_review_html_url = event.review.html_url || '';
        }

        // Repository
        if (event.repository) {
            vars.default_branch = event.repository.default_branch || 'main';
        }
    }

    return vars;
}

/**
 * Apply frontmatter defaults with variable interpolation support
 */
function applyFrontmatterDefaults(frontmatter, baseVars) {
    const result = { ...baseVars };

    // Process frontmatter keys in order, allowing later ones to reference earlier ones
    for (const [key, value] of Object.entries(frontmatter)) {
        if (typeof value === 'string') {
            // Interpolate variables in frontmatter values
            result[key] = renderTemplate(value, result);
        } else {
            result[key] = value;
        }
    }

    return result;
}

/**
 * Load and render a partial template
 * Resolution order: partial frontmatter defaults < parent vars < explicit overrides
 */
function loadPartial(name, parentVars, overrideParams = {}) {
    const partialPath = path.join(
        __dirname,
        '..',
        'templates',
        'partials',
        `${name}.md`,
    );

    if (!fs.existsSync(partialPath)) {
        throw new Error(
            `Partial not found: ${name} (looked at ${partialPath})`,
        );
    }

    const content = fs.readFileSync(partialPath, 'utf8');
    const { data: partialFrontmatter, content: partialBody } = matter(content);

    // Resolution order: partial defaults < parent vars < explicit overrides
    const mergedVars = {
        ...partialFrontmatter, // lowest: partial's own defaults
        ...parentVars, // middle: inherited from parent
        ...overrideParams, // highest: explicit include() params
    };

    // Render the partial with merged variables (recursive to support nested includes)
    return renderTemplate(partialBody, mergedVars);
}

/**
 * Extract balanced expression from template starting at position
 * Handles nested braces properly
 * startPos should point to the first character after '${'
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
                    // Found the closing brace of ${ ... }
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
 * Render template by replacing ${var_name} with values
 * Supports basic conditionals: ${condition ? 'true' : 'false'}
 * Supports include('partial-name') and include('partial-name', { overrides })
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
 * Evaluate a single expression
 */
function evaluateExpression(expression, vars) {
    try {
        // Simple variable reference
        if (/^[a-zA-Z_][a-zA-Z0-9_]*$/.test(expression.trim())) {
            const value = vars[expression.trim()];
            return value !== undefined && value !== null ? String(value) : '';
        }

        // Expression evaluation (ternary, concatenation, etc.)
        // Create a safe evaluation context
        const evalContext = { ...vars };

        // Add helper functions
        evalContext.derive_package_name = (packagePath) => {
            if (!packagePath || packagePath === '.') return '';

            // Try to read package name from Cargo.toml or package.json
            try {
                const cargoPath = path.join(
                    process.cwd(),
                    packagePath,
                    'Cargo.toml',
                );
                if (fs.existsSync(cargoPath)) {
                    const cargoContent = fs.readFileSync(cargoPath, 'utf8');
                    const match = cargoContent.match(
                        /^\[package\][\s\S]*?name\s*=\s*"([^"]+)"/m,
                    );
                    if (match) return match[1];
                }
            } catch {}

            try {
                const pkgPath = path.join(
                    process.cwd(),
                    packagePath,
                    'package.json',
                );
                if (fs.existsSync(pkgPath)) {
                    const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
                    if (pkg.name) return pkg.name;
                }
            } catch {}

            // Fallback to directory name
            return packagePath.split('/').pop() || '';
        };

        // Add include() helper for composable partials
        evalContext.include = (partialName, overrideParams = {}) => {
            return loadPartial(partialName, vars, overrideParams);
        };

        // Build safe evaluation function
        const evalFn = new Function(
            ...Object.keys(evalContext),
            `return ${expression};`,
        );
        const result = evalFn(...Object.values(evalContext));

        return result !== undefined && result !== null ? String(result) : '';
    } catch (error) {
        console.warn(
            `⚠️ Failed to evaluate expression: ${expression}`,
            error.message,
        );
        return '${' + expression + '}';
    }
}

/**
 * Output results to GitHub Actions outputs
 */
function outputResults(renderedPrompt, resolvedVars) {
    const outputFile = process.env.GITHUB_OUTPUT;

    if (!outputFile) {
        console.error('❌ GITHUB_OUTPUT environment variable not set');
        process.exit(1);
    }

    // Write rendered prompt to output
    fs.appendFileSync(
        outputFile,
        `rendered_prompt<<EOF\n${renderedPrompt}\nEOF\n`,
    );

    // Write resolved vars as JSON
    const varsJson = JSON.stringify(resolvedVars);
    fs.appendFileSync(outputFile, `resolved_vars<<EOF\n${varsJson}\nEOF\n`);

    // Write specific useful outputs
    if (resolvedVars.branch_name) {
        fs.appendFileSync(
            outputFile,
            `branch_name=${resolvedVars.branch_name}\n`,
        );
    }

    if (resolvedVars.commit_message) {
        fs.appendFileSync(
            outputFile,
            `commit_message=${resolvedVars.commit_message}\n`,
        );
    }

    console.log('✅ Template resolved successfully');
    console.log(`📊 Resolved ${Object.keys(resolvedVars).length} variables`);
}

main();
