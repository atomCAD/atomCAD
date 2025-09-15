use std::path::Path;

/// Utilities for handling file paths across different operating systems
/// Supports Windows, Linux, and macOS path conventions

/// Determines if a path is absolute
/// Works across Windows (C:\, \\server\share), Unix-like systems (/path), and UNC paths
pub fn is_absolute_path(path: &str) -> bool {
    let path_obj = Path::new(path);
    path_obj.is_absolute()
}

/// Converts an absolute path to a relative path based on a base directory
/// Returns None if the path is not under the base directory or if conversion fails
pub fn absolute_to_relative(absolute_path: &str, base_dir: &str) -> Option<String> {
    let abs_path = Path::new(absolute_path);
    let base_path = Path::new(base_dir);
    
    // Ensure both paths are absolute
    if !abs_path.is_absolute() || !base_path.is_absolute() {
        return None;
    }
    
    // Canonicalize paths to handle symlinks and normalize separators
    let abs_canonical = abs_path.canonicalize().ok()?;
    let base_canonical = base_path.canonicalize().ok()?;
    
    // Try to create relative path
    match abs_canonical.strip_prefix(&base_canonical) {
        Ok(relative) => Some(relative.to_string_lossy().to_string()),
        Err(_) => None, // Path is not under base directory
    }
}

/// Converts a relative path to an absolute path based on a base directory
/// Returns None if the base directory doesn't exist or conversion fails
pub fn relative_to_absolute(relative_path: &str, base_dir: &str) -> Option<String> {
    let rel_path = Path::new(relative_path);
    let base_path = Path::new(base_dir);
    
    // Ensure relative path is actually relative
    if rel_path.is_absolute() {
        return None;
    }
    
    // Ensure base directory exists and is absolute
    if !base_path.is_absolute() || !base_path.exists() {
        return None;
    }
    
    // Join paths and canonicalize
    let combined = base_path.join(rel_path);
    combined.canonicalize().ok()?.to_str().map(|s| s.to_string())
}

/// Resolves a path that could be either relative or absolute
/// If relative, converts to absolute using base_dir
/// If absolute, returns as-is if it exists
/// Returns the resolved absolute path and whether it was originally relative
pub fn resolve_path(path: &str, base_dir: Option<&str>) -> Result<(String, bool), String> {
    if is_absolute_path(path) {
        // Path is absolute - verify it exists
        let path_obj = Path::new(path);
        if path_obj.exists() {
            Ok((path.to_string(), false))
        } else {
            Err(format!("Absolute path does not exist: {}", path))
        }
    } else {
        // Path is relative - need base directory
        match base_dir {
            Some(base) => {
                match relative_to_absolute(path, base) {
                    Some(absolute) => Ok((absolute, true)),
                    None => Err(format!("Failed to resolve relative path '{}' with base '{}'", path, base)),
                }
            }
            None => Err("Relative path provided but no base directory available".to_string()),
        }
    }
}

/// Attempts to convert an absolute path to relative if it's under the base directory
/// If conversion is possible, returns the relative path and updates should_store_relative to true
/// If not possible or path is already relative, returns the original path
pub fn try_make_relative(path: &str, base_dir: Option<&str>) -> (String, bool) {
    match base_dir {
        Some(base) if is_absolute_path(path) => {
            match absolute_to_relative(path, base) {
                Some(relative) => (relative, true),
                None => (path.to_string(), false),
            }
        }
        _ => (path.to_string(), false),
    }
}

/// Gets the directory containing a file path
/// Works with both absolute and relative paths
pub fn get_parent_directory(file_path: &str) -> Option<String> {
    Path::new(file_path)
        .parent()
        .and_then(|p| p.to_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    
    #[test]
    fn test_is_absolute_path() {
        // Unix-style absolute paths
        assert!(is_absolute_path("/home/user/file.txt"));
        assert!(is_absolute_path("/"));
        
        // Windows-style absolute paths
        if cfg!(windows) {
            assert!(is_absolute_path("C:\\Users\\file.txt"));
            assert!(is_absolute_path("D:\\"));
            assert!(is_absolute_path("\\\\server\\share\\file.txt"));
        }
        
        // Relative paths
        assert!(!is_absolute_path("file.txt"));
        assert!(!is_absolute_path("./file.txt"));
        assert!(!is_absolute_path("../file.txt"));
        assert!(!is_absolute_path("folder/file.txt"));
    }
    
    #[test]
    fn test_get_parent_directory() {
        assert_eq!(get_parent_directory("/home/user/file.txt"), Some("/home/user".to_string()));
        assert_eq!(get_parent_directory("folder/file.txt"), Some("folder".to_string()));
        assert_eq!(get_parent_directory("file.txt"), Some("".to_string()));
        
        if cfg!(windows) {
            assert_eq!(get_parent_directory("C:\\Users\\file.txt"), Some("C:\\Users".to_string()));
        }
    }
    
    #[test]
    fn test_try_make_relative() {
        // Test with no base directory
        let (result, changed) = try_make_relative("/some/path/file.txt", None);
        assert_eq!(result, "/some/path/file.txt");
        assert!(!changed);
        
        // Test with relative path (should return unchanged)
        let (result, changed) = try_make_relative("relative/file.txt", Some("/base"));
        assert_eq!(result, "relative/file.txt");
        assert!(!changed);
    }
}
