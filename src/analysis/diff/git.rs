use super::*;

// ============================================================================
// Git Integration
// ============================================================================

/// Validate git revision string to prevent command injection
pub(super) fn validate_git_revision(revision: &str) -> Result<(), io::Error> {
    // Allow: branch names, commit SHAs, HEAD~N, tags, etc.
    // Pattern: alphanumeric, dash, underscore, slash, tilde, caret, at, colon, dot
    let valid_pattern = Regex::new(r"^[a-zA-Z0-9_\-/.~^@:]+$")
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Regex error: {e}")))?;

    if !valid_pattern.is_match(revision) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid git revision format: {}", revision),
        ));
    }

    // Additional check: reject suspicious patterns
    if revision.contains("..") && !revision.contains("...") {
        // Allow "..." (three dots) for merge base, but be cautious with ".." (two dots)
        log::warn!("Git revision contains '..' which may be a range operator");
    }

    Ok(())
}

/// Get the old version of a file from git
pub(super) fn get_git_file_content(file_path: &Path, revision: &str) -> Result<String, io::Error> {
    // Validate revision to prevent command injection
    validate_git_revision(revision)?;

    // Construct the git show command: git show <revision>:<path>
    let repo_relative_path = get_repo_relative_path(file_path)?;

    let output = Command::new("git")
        .args(["show", &format!("{}:{}", revision, repo_relative_path)])
        .current_dir(file_path.parent().unwrap_or(Path::new(".")))
        .output()
        .map_err(|e| io::Error::other(format!("Failed to execute git: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Git show failed: {stderr}"),
        ));
    }

    String::from_utf8(output.stdout).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid UTF-8 in git output: {e}"),
        )
    })
}

/// Get the repository-relative path for a file
pub(super) fn get_repo_relative_path(file_path: &Path) -> Result<String, io::Error> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(file_path.parent().unwrap_or(Path::new(".")))
        .output()
        .map_err(|e| io::Error::other(format!("Failed to get git root: {e}")))?;

    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Not a git repository",
        ));
    }

    let repo_root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let repo_root = Path::new(&repo_root);

    let canonical_file = file_path.canonicalize()?;
    let relative = canonical_file.strip_prefix(repo_root).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "File is not within the git repository",
        )
    })?;

    Ok(relative.to_string_lossy().to_string())
}

/// Resolve a git revision to its full SHA
pub(super) fn resolve_git_sha(revision: &str, file_path: &Path) -> Result<String, io::Error> {
    validate_git_revision(revision)?;

    let output = Command::new("git")
        .args(["rev-parse", revision])
        .current_dir(file_path.parent().unwrap_or(Path::new(".")))
        .output()
        .map_err(|e| io::Error::other(format!("Failed to resolve git revision: {e}")))?;

    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Could not resolve revision: {}", revision),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
