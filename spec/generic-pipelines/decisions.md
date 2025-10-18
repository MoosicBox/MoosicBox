# Generic Pipelines - Architectural Decisions

Record of key decisions made during specification refinement.

## ADR-001: Sequential Phase Execution

**Date:** 2025-01-09
**Status:** Accepted
**Decision:** All phases execute sequentially, must complete before next begins
**Rationale:** Simplifies dependency management and ensures clean architecture boundaries

## ADR-002: Action Resolution via Workflow Mapping

**Date:** 2025-01-09
**Status:** Accepted
**Decision:** Actions defined in top-level `actions:` property with explicit type
**Rationale:** Explicit mapping gives users full control over action resolution

## ADR-003: Hard Fail on Missing Translations

**Date:** 2025-01-09
**Status:** Accepted
**Decision:** No fallback when backend translation missing - hard error
**Rationale:** Explicit failures prevent silent incorrect behavior

## ADR-004: String-Only Outputs

**Date:** 2025-01-09
**Status:** Accepted
**Decision:** All outputs are strings, matching GitHub Actions model
**Rationale:** Simplicity and compatibility with existing workflows

## ADR-005: Sequential Local Execution

**Date:** 2025-01-09
**Status:** Accepted
**Decision:** Jobs and matrix runs execute sequentially in local runner
**Rationale:** Reduces initial complexity, parallelism can be added later

## ADR-006: Environment Variable Secrets

**Date:** 2025-01-09
**Status:** Accepted
**Decision:** Use `PIPELINE_SECRET_*` environment variables for local secrets
**Rationale:** Simple, secure, and familiar pattern for developers

## ADR-007: Backend Detection Strategy

**Date:** 2025-01-09
**Status:** Accepted
**Decision:** CLI flag `--backend=<name>` with environment auto-detection as fallback
**Rationale:** Explicit control when needed, sensible defaults for common usage

## ADR-008: Conditional Translation Rules

**Date:** 2025-01-09
**Status:** Accepted
**Decision:** Simple conditionals stripped during translation, complex ones become false
**Rationale:** Optimizes simple cases while handling complex logic correctly

## ADR-009: GitHub Actions Compatibility

**Date:** 2025-01-09
**Status:** Accepted
**Decision:** Match GitHub's outcome vs conclusion error handling model exactly
**Rationale:** Ensures familiar behavior for users migrating from GitHub Actions

## ADR-010: Pipeline Output File

**Date:** 2025-01-09
**Status:** Accepted
**Decision:** Use `$PIPELINE_OUTPUT` file for step outputs, similar to `$GITHUB_OUTPUT`
**Rationale:** Familiar pattern for GitHub Actions users, simple implementation

## ADR-011: Circular Dependency Validation

**Date:** 2025-01-09
**Status:** Accepted
**Decision:** Validate job dependency DAG at parse time to prevent cycles
**Rationale:** Fail fast with clear error messages rather than runtime deadlock

## ADR-012: Backend-Agnostic Triggers

**Date:** 2025-01-09
**Status:** Accepted
**Decision:** Generic format uses backend-agnostic trigger names, translated per backend
**Rationale:** Enables true cross-platform workflows while maintaining backend compatibility

## ADR-013: Trigger Handling for Local Execution

**Date:** 2025-01-09
**Status:** Accepted
**Decision:** Ignore workflow triggers for local execution, stub event context
**Rationale:** Local execution focuses on workflow logic, not event-driven execution
