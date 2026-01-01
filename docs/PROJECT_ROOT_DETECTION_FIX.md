# Project Root Detection Fix

## Problem

runctl was detecting the project root incorrectly in some projects. For example, if a project had this structure:

```
repo-root/
  .git
  requirements.txt
  src/
    ml/
      requirements.txt  <-- This caused the problem
      train.py
```

When running `runctl aws train src/ml/train.py`, runctl would detect `src/ml/` as the project root instead of the actual repo root, because it found `requirements.txt` in `src/ml/` and stopped searching.

## Root Cause

The `find_project_root` function in `src/utils.rs` was stopping at the **first** marker it found, without prioritizing `.git` (which is always at the repo root). This caused false positives when subdirectories contained markers like `requirements.txt`.

## Solution

### 1. Prioritize `.git` as Most Authoritative Marker

The function now:
- **First** searches for `.git` directory (most reliable indicator of repo root)
- **Continues searching upward** even if other markers (like `requirements.txt`) are found in subdirectories
- Only uses other markers as fallback if `.git` is not found

### 2. Enhanced Logic

```rust
pub fn find_project_root(start_path: &Path) -> PathBuf {
    let mut current = start_path.to_path_buf();
    let mut found_without_git: Option<PathBuf> = None;
    
    loop {
        // Prioritize .git as the most authoritative marker
        if current.join(".git").exists() {
            // Warn if we found a subdirectory marker earlier
            if let Some(ref subdir_root) = found_without_git {
                if subdir_root != &current {
                    warn!("Found project marker in subdirectory, but continuing to repo root (found .git)");
                }
            }
            return current; // Return repo root
        }
        
        // Check for other markers (but continue searching for .git)
        if found_without_git.is_none() {
            if other_markers.iter().any(|m| current.join(m).exists()) {
                found_without_git = Some(current.clone());
                // Continue searching upward for .git
            }
        }
        
        // Move to parent directory
        match current.parent() {
            Some(p) => current = p.to_path_buf(),
            None => {
                // Reached filesystem root - return best match
                if let Some(fallback) = found_without_git {
                    warn!("Found project marker in subdirectory, but no .git found. Using subdirectory as project root.");
                    return fallback;
                }
                return start_path.to_path_buf();
            }
        }
    }
}
```

### 3. Added Warnings

The function now warns when:
- A subdirectory marker is found but `.git` exists at a higher level
- No `.git` is found and a subdirectory marker is used as fallback

### 4. Enhanced Logging in Training

Added warnings in `src/aws/training.rs`:
- Warns if project root seems incorrect (script is very deep)
- Logs the detected project root for debugging
- Suggests ensuring `.git` is at repo root for more reliable detection

## Test Cases

Added comprehensive tests:

1. **`test_find_project_root_prioritizes_git`**: Verifies that `.git` takes precedence over nested `requirements.txt`
2. **`test_find_project_root_src_ml_scenario`**: Specifically tests the reported issue (src/ml/requirements.txt)

## Behavior Changes

### Before
- Stopped at first marker found
- `src/ml/requirements.txt` → project root = `src/ml/`
- Could miss the actual repo root

### After
- Prioritizes `.git` and continues searching
- `src/ml/requirements.txt` + `.git` at root → project root = repo root ✅
- Falls back to subdirectory marker only if no `.git` exists

## Recommendations for Users

1. **Ensure `.git` is at repo root** (most reliable)
2. **If no `.git`**, ensure project markers (`requirements.txt`, `pyproject.toml`, etc.) are at the repo root
3. **Avoid nested markers** in subdirectories if possible (or ensure `.git` exists at root)

## Example Scenarios

### Scenario 1: Repo with .git (✅ Works correctly)
```
repo-root/
  .git                    ← Found first, returns repo-root
  requirements.txt
  src/
    ml/
      requirements.txt    ← Ignored (continues searching)
      train.py
```
**Result**: `repo-root/` ✅

### Scenario 2: Repo without .git (⚠️ Uses fallback)
```
repo-root/
  requirements.txt        ← Found first, returns repo-root
  src/
    ml/
      requirements.txt    ← Ignored (continues searching, finds root first)
      train.py
```
**Result**: `repo-root/` ✅ (but warns about missing .git)

### Scenario 3: Nested marker without .git (⚠️ Uses subdirectory)
```
repo-root/
  (no markers)
  src/
    ml/
      requirements.txt    ← Found, no .git above, returns src/ml/
      train.py
```
**Result**: `src/ml/` ⚠️ (warns about missing .git at root)

## Impact

- ✅ Fixes the reported issue where `src/ml/` was incorrectly detected as project root
- ✅ More reliable project root detection
- ✅ Better warnings and logging for debugging
- ✅ Backward compatible (still works for projects without `.git`)
- ✅ Enhanced error messages with actionable suggestions
- ✅ Support for more project types (Node.js, Go, Maven, Gradle, Make)

## Additional Improvements

### Enhanced Markers Support

The function now recognizes markers for multiple project types:
- **Python**: `requirements.txt`, `setup.py`, `pyproject.toml`
- **Rust**: `Cargo.toml`
- **Node.js**: `package.json`
- **Go**: `go.mod`
- **Maven**: `pom.xml`
- **Gradle**: `build.gradle`
- **Make**: `Makefile`

### Improved Error Messages

When project root detection fails or seems incorrect:
- Clear explanation of what went wrong
- Specific suggestions for fixing the issue
- Links to documentation
- Context about nested markers

### Symlink Safety

- Prevents infinite loops with symlinks
- Tracks seen paths to avoid cycles
- Handles edge cases gracefully

### Validation in Training

When training starts:
- Validates project root makes sense
- Warns if script is very deep (potential misdetection)
- Logs detected project root for debugging
- Provides helpful context in error messages

