# Implementation Ready Checklist

## ✅ All Critical Decisions Resolved

The Generic Pipelines (gpipe) project is now ready for implementation. All ambiguities from the original specification have been resolved through comprehensive Q&A sessions.

## Implementation Decisions Captured

### 1. **AST Structure** ✅
- Concrete Rust type definitions provided
- Step enum with UseAction/RunScript variants
- Expression AST with all node types defined
- BTreeMap for deterministic ordering

### 2. **Expression Language** ✅
- MVP function set: `toJson()`, `fromJson()`, `contains()`, `startsWith()`, `join()`, `format()`
- Operators: `==`, `!=`, `&&`, `||`, `!`, property access
- No status functions for MVP
- Complete Expression enum structure

### 3. **Package Structure** ✅
- Umbrella crate: `packages/gpipe/`
- Sub-crates: `gpipe_ast`, `gpipe_parser`, `gpipe_runner`, `gpipe_translator`, `gpipe_actions`, `gpipe_cli`
- Binary name: `gpipe`
- Follows MoosicBox patterns (switchy/hyperchad)

### 4. **Built-in Actions** ✅
- File-based in `.pipeline/actions/` directory
- No embedded actions in binary
- Standard YAML format like user actions
- checkout, setup-*, upload-artifact as files

### 5. **CLI Commands** ✅
```bash
gpipe run workflow.yml [--backend=local] [--secret KEY=val] [--env KEY=val] [--dry-run]
gpipe translate workflow.yml --target=github [--output=path]
gpipe validate workflow.yml
```

### 6. **Workflow Format** ✅
- Complete YAML schema defined
- GitHub-compatible expression syntax
- Backend conditionals with constant replacement
- Step outputs via `$PIPELINE_OUTPUT`

### 7. **Execution Semantics** ✅
- Sequential job execution locally
- Current OS only for matrix
- Outcome/conclusion error handling
- Failed jobs block dependents

## Next Steps

1. **Create Package Structure**: Set up `packages/gpipe/` with sub-crates
2. **Implement `gpipe_ast`**: Start with core types and Expression enum
3. **Implement `gpipe_parser`**: Begin with Generic format parsing
4. **Implement `gpipe_runner`**: Local execution engine
5. **Implement `gpipe_cli`**: Basic run/translate/validate commands

## No Remaining Ambiguities

All major design decisions have been documented with concrete specifications. The team can begin implementation immediately with clear guidance.

**Status**: 🟢 **READY FOR DEVELOPMENT**
