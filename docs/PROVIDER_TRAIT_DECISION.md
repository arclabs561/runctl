# Provider Trait System Decision

**Date**: 2025-01-03  
**Status**: Documented Decision

## Current State

The `TrainingProvider` trait is fully defined with implementations:
- `AwsProvider` in `src/providers/aws_provider.rs`
- `RunPodProvider` in `src/providers/runpod_provider.rs`
- `LyceumProvider` in `src/providers/lyceum_provider.rs`

However, the CLI (`src/main.rs`) bypasses this abstraction and calls `aws::handle_command()` directly.

## Decision: Keep Direct Implementation for Now

### Rationale

1. **Working Code**: The direct AWS implementation in `aws.rs` is functional, tested, and handles all edge cases
2. **Provider Implementations Incomplete**: Provider trait implementations are skeletons that return placeholder errors
3. **Migration Risk**: Refactoring 2689 lines of working code to use the trait system is high-risk
4. **No Immediate Need**: Current use case is AWS-only; multi-cloud support is future work

### Trade-offs

**Pros of Current Approach:**
- ✅ Working, tested code
- ✅ No abstraction overhead
- ✅ Direct control over AWS-specific features
- ✅ Easier to debug (no trait indirection)

**Cons of Current Approach:**
- ❌ Can't easily switch providers
- ❌ Code duplication between `aws.rs` and `providers/aws_provider.rs`
- ❌ Harder to test (can't mock providers)
- ❌ Violates stated architecture principle

## Future Migration Path

When multi-cloud support becomes a priority:

1. **Complete Provider Implementations**
   - Finish `AwsProvider` by refactoring `aws.rs` logic
   - Complete `RunPodProvider` and `LyceumProvider`
   - Add comprehensive tests

2. **Gradual Migration**
   - Start with new commands using provider trait
   - Migrate existing commands incrementally
   - Keep both systems working during transition

3. **Provider Registry**
   - Add provider registry in `src/providers/mod.rs`
   - CLI selects provider based on command/flag
   - Default to AWS for backward compatibility

## Recommendation

**Keep the provider trait system** (don't delete it) but **don't force migration** until:
- Multi-cloud support is actually needed
- Provider implementations are complete
- Migration can be done incrementally without breaking existing functionality

## Code Status

- `src/provider.rs`: ✅ Well-defined trait
- `src/providers/aws_provider.rs`: ⚠️ Skeleton (placeholder errors)
- `src/providers/runpod_provider.rs`: ⚠️ Skeleton (placeholder errors)
- `src/providers/lyceum_provider.rs`: ⚠️ Skeleton (placeholder errors)
- `src/aws.rs`: ✅ Full implementation (2689 lines)
- `src/main.rs`: ✅ Uses direct `aws::handle_command()`

## Action Items

- [ ] Mark provider implementations with `#[allow(dead_code)]` and document why
- [ ] Add TODO comments explaining future migration path
- [ ] Update `.cursorrules` to reflect current decision
- [ ] Consider adding integration tests for provider trait (even if unused)

