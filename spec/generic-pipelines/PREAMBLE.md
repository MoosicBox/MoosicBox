# Generic Pipelines - Preamble

A universal CI/CD workflow orchestration tool that can generate and execute workflows across multiple backends (GitHub Actions, GitLab CI, local execution) without requiring containerization for local runs. The tool provides a common abstraction layer over different CI systems while maintaining the ability to run workflows imperatively as if executing them manually.

## Context

- Specs use checkboxes (`- [ ]`) to track progress
- Four-phase workflow: preliminary check → deep analysis → execution → verification
- NO COMPROMISES - halt on any deviation from spec
    - Includes comprehensive test coverage for all business logic
    - Tests must be written alongside implementation, not deferred
    - Both success and failure paths must be tested
- Living documents that evolve during implementation
- After having completed a checkbox, 'check' it and add details under it regarding the file/location updated as PROOF

See `generic-pipelines/plan.md` for the current status of the generic-pipelines and what's next to be done.
