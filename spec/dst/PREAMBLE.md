# DST Specification Working Guide

## Purpose

This guide shapes how AI agents should approach discussions and work on the MoosicBox DST (Deterministic Simulation Testing) specification to maximize value, maintain focus, and properly document progress.

## Core Principles

### The Spec is a Living Work Log

The DST spec (@spec/dst/overview.md) is not just documentation - it's an active audit and work tracking system that:

- Tracks the transformation of non-deterministic patterns into deterministic ones
- Serves as the single source of truth for this effort
- Documents both what was done and how it was validated

### Document Your Work Thoroughly

Every checkbox marked as complete MUST include:

1. **Location details** - Specific files, line numbers, packages affected
2. **Implementation evidence** - What was done and how
3. **Validation proof** - How you verified it works

## Discussion Framework

### 1. Start with Orientation

Before diving into any specific topic, establish context:

- What aspect of determinism are we discussing?
- Is this about identifying problems, tracking progress, or implementing solutions?
- Are we working within existing patterns or proposing new approaches?

### 2. Respect the Document Structure

The spec follows an audit pattern:

- **Problem identification** → What needs to be fixed
- **Status tracking** → Current state of each item
- **Solution documentation** → How things were/will be fixed
- **Execution planning** → Order and dependencies

Always frame discussions within this structure rather than creating parallel taxonomies.

### 3. Use Precise Language

- **"Fixed"** means completely resolved with evidence
- **"In Progress"** means actively being worked on with partial completion noted
- **"Blocked"** means waiting on dependencies (document what's blocking)
- **"Pattern"** refers to a reusable solution approach
- **"Migration"** refers to converting existing code

### 4. Follow the Abstraction Hierarchy

Discussions should respect these levels:

1. **Architectural** - Overall approach and principles
2. **Category** - Types of non-determinism (collections, time, etc.)
3. **Package** - Specific codebase modules
4. **Implementation** - Actual code changes

Start discussions at the appropriate level and explicitly move between levels.

## The Checkbox Protocol

### Good Documentation Example

```markdown
- [x] `packages/database_connection/src/creds.rs:38-78` - Database credentials ✅ COMPLETED
    - Line 38: DATABASE_URL migrated using var_or("DATABASE_URL", "sqlite::memory:")
    - Lines 44-47: DB_HOST, DB_NAME, DB_USER, DB_PASSWORD using var_parse_or
    - Validation: Compiled with zero warnings, tested with simulator feature
    - Pattern: Used switchy_env for dual-mode environment variables
```

### Poor Documentation Example

```markdown
- [x] Fixed database credentials ✅
```

## Working Patterns

### Pattern: Status Check

"What's the state of X?"

1. Locate X in the spec
2. Report its status marker with completion percentage if applicable
3. Identify dependencies/blockers
4. Reference the documented evidence of completion

### Pattern: Solution Proposal

"How should we handle Y?"

1. Check if Y is already addressed (look for ✅ markers)
2. Find similar solved problems (look for patterns in completed work)
3. Propose applying existing patterns (reference specific examples)
4. Only suggest new patterns if necessary

### Pattern: Work Planning

"What should we do next?"

1. Reference current phase/progress with percentages
2. Identify incomplete items (look for ⏳ markers)
3. Check for unblocked work (avoid ❌ items)
4. Consider parallel opportunities

### Pattern: Problem Discovery

"I found non-determinism in Z"

1. Categorize the type of non-determinism
2. Check if it's already documented
3. Assess impact and priority
4. Add to spec with proper structure if genuinely new

## The "Switchy" Mental Model

The core pattern for determinism is dual-mode packages:

- **Production mode** - Normal, non-deterministic behavior
- **Simulation mode** - Deterministic, reproducible behavior
- **Feature flags** - Compile-time switching between modes

Frame solutions within this model. When documenting implementations, show how both modes are handled.

## Documentation Standards

### File References

Always use: `packages/[package_name]/src/[file].rs:[line_numbers]`

### Status Progression

- ❌ Blocked → Document what's blocking and why
- ⏳ In Progress → Show percentage complete and what's done
- ✅ Complete → Include full implementation details and validation

### Migration Documentation

```markdown
- [x] `packages/[name]/src/[file].rs:[lines]` - [What] ✅ MIGRATED
    - Before: `std::env::var("FOO")`
    - After: `switchy_env::var("FOO")`
    - Validation: [How verified]
    - Pattern: [Reusable approach]
```

## Meta-Principles

### The Spec is Authoritative

- If it's not in the spec, it's not part of the DST effort
- If it contradicts the spec, the spec wins
- If the spec is wrong, update the spec first with evidence

### Progress Over Perfection

- Partial determinism is better than none
- Document partial completions with percentages
- Quick wins build momentum

### Patterns Over Instances

- Look for reusable solutions in completed work
- Document patterns when you find them
- Enable parallel work through patterns

### Explicit Over Implicit

- Document blockers clearly
- State assumptions explicitly
- Make dependencies visible
- Include validation evidence

## Quality Checklist

Before marking any task complete:

- [ ] Documented all affected files with paths and line numbers
- [ ] Described what was actually implemented
- [ ] Noted how it was validated/tested
- [ ] Identified any patterns for reuse
- [ ] Updated percentages if applicable
- [ ] Marked follow-up work if needed

## How This Guide Helps

By following this framework, work on the DST spec will:

- Build on existing patterns rather than reinventing solutions
- Create an audit trail of changes and validations
- Enable parallel work through clear documentation
- Maintain clear communication about progress
- Provide evidence of completion
- Build a knowledge base for future work

The goal is not just to check boxes, but to create a comprehensive record of a complex technical transformation where every change is documented, validated, and reusable.

**Remember: The spec is only as valuable as the completion details it contains.**
