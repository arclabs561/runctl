# Naming Research: runctl Tool & Directory

## Current State

- **Tool name**: `runctl` ✅
- **Directory name**: `infra-utils` ❌
- **Package name**: `runctl` (in Cargo.toml) ✅

## Analysis

### Tool Name: `runctl` ✅ **KEEP**

**Strengths:**
- ✅ Short and memorable (9 characters)
- ✅ Descriptive: clearly indicates "training operations"
- ✅ Follows CLI conventions: lowercase, kebab-case
- ✅ Action-oriented: "ops" = operations
- ✅ Avoids generic words: no "tool", "kit", "util"
- ✅ Not taken: GitHub search shows no major conflicts
- ✅ Professional: similar to established tools (`kubectl`, `gh`, `aws`)

**Comparison to Best Practices:**
- ✅ Short enough for frequent use
- ✅ Clear purpose (ML training orchestration)
- ✅ Memorable and easy to type
- ✅ Follows established patterns

### Directory Name: `infra-utils` ❌ **CHANGE**

**Problems:**
- ❌ Too generic: "utils" is discouraged in CLI naming
- ❌ Doesn't match tool name: creates confusion
- ❌ Misleading: suggests general infrastructure utilities, not ML training
- ❌ Not descriptive: doesn't indicate the tool's purpose

**Recommendation:** Change to `runctl` to match the tool name.

## Research Findings

### CLI Naming Best Practices

1. **Keep names short and memorable**
   - Very short names (2-3 chars) for frequently used tools
   - Longer names (8-12 chars) acceptable for niche tools
   - `runctl` (9 chars) is in the sweet spot

2. **Avoid generic words**
   - ❌ "tool", "kit", "util", "easy"
   - ✅ Specific, descriptive terms

3. **Use action-oriented names**
   - Verbs or action nouns preferred
   - "ops" = operations (action-oriented)

4. **Follow conventions**
   - Lowercase only
   - Kebab-case for multi-word names
   - No spaces or special characters

### Similar Tools Analysis

| Tool | Name | Length | Pattern |
|------|------|--------|---------|
| `kubectl` | Kubernetes control | 7 | `kube` + `ctl` |
| `gh` | GitHub CLI | 2 | Abbreviation |
| `aws` | AWS CLI | 3 | Abbreviation |
| `docker` | Docker CLI | 6 | Product name |
| `runctl` | Training ops | 9 | `train` + `ops` |

**Conclusion:** `runctl` fits well with established patterns.

## Recommendations

### ✅ Keep `runctl` as tool name
- Already follows best practices
- Clear, memorable, professional
- No conflicts found

### ✅ Change directory from `infra-utils` to `runctl`
- Matches tool name (consistency)
- More descriptive
- Avoids generic "utils" term
- Clearer purpose

### Migration Path

1. **Rename directory:**
   ```bash
   cd /Users/arc/Documents/dev
   mv infra-utils runctl
   ```

2. **Update references:**
   - Workspace paths (if any)
   - Documentation
   - CI/CD workflows
   - Any hardcoded paths

3. **Verify:**
   - All imports still work
   - Tests pass
   - Documentation updated

## Alternative Names Considered

### For Tool (if changing):
- `runctl` - Similar to `kubectl`, but less clear
- `mltrain` - Too generic, doesn't indicate orchestration
- `train-orchestrator` - Too long (18 chars)
- `runctl` - Unclear abbreviation

**Verdict:** `runctl` is the best option.

### For Directory:
- `runctl` ✅ - Matches tool name (recommended)
- `ml-training-ops` - Too long
- `training-cli` - Generic, doesn't match tool name

## Conclusion

**Final Recommendation:**
- **Tool name**: Keep `runctl` ✅
- **Directory name**: Change to `runctl` ✅
- **Package name**: Already `runctl` ✅

This creates consistency across all naming and follows CLI best practices.

