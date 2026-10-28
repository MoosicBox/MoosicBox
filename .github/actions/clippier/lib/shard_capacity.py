#!/usr/bin/env python3
"""Single source of truth for statically wired GitHub hierarchy capacity."""

GITHUB_WORKFLOW_LEVELS = 10  # Includes the top-level caller.
GITHUB_UNIQUE_WORKFLOWS = 50
EXECUTOR_LEVELS = 1
MAX_GROUP_LEVELS = GITHUB_WORKFLOW_LEVELS - 1 - EXECUTOR_LEVELS


def capacity(caller_depth=1):
    """The caller depth counts the workflow invoking our first dispatcher."""
    if type(caller_depth) is not int or not 1 <= caller_depth <= MAX_GROUP_LEVELS:
        raise ValueError(
            f"shard-caller-depth must be an integer from 1 to {MAX_GROUP_LEVELS}"
        )
    return GITHUB_WORKFLOW_LEVELS - caller_depth - EXECUTOR_LEVELS


def dispatcher_name(index):
    if index == 1:
        return "clippier-build-group.yml"
    return f"clippier-build-group-{index}.yml"
