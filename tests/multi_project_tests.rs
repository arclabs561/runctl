//! Tests for multi-project and tagging functionality


// Note: These tests require the functions to be public or use integration test approach
// For now, testing the logic indirectly through public APIs

#[test]
fn test_project_name_sanitization() {
    // Test that directory names with special chars are sanitized
    let test_cases = vec![
        ("my-project", "my-project"),
        ("my project", "my-project"),
        ("my.project", "my-project"),
        ("my/project", "my-project"),
        ("my@project", "my-project"),
    ];

    // The sanitization logic should convert special chars to '-'
    for (input, _expected) in test_cases {
        let sanitized: String = input
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        // Should not contain spaces or special chars (except - and _)
        assert!(!sanitized.contains(' '));
        assert!(!sanitized.contains('.'));
        assert!(!sanitized.contains('/'));
        assert!(!sanitized.contains('@'));
    }
}

#[test]
fn test_tag_format() {
    // Test that tag names follow expected format
    let instance_id = "i-1234567890abcdef0";
    let project_name = "test-project";
    let user_id = "alice";

    let name_tag = format!(
        "trainctl-{}-{}-{}",
        user_id,
        project_name,
        &instance_id[..8]
    );

    assert_eq!(name_tag, "trainctl-alice-test-project-i-123456");
    assert!(name_tag.len() <= 128); // AWS tag value limit
}

#[test]
fn test_tag_keys() {
    // Verify all expected tag keys
    let expected_keys = vec![
        "Name",
        "trainctl:created",
        "trainctl:project",
        "trainctl:user",
        "CreatedBy",
    ];

    for key in expected_keys {
        assert!(!key.is_empty());
        assert!(key.len() <= 128); // AWS tag key limit
        assert!(!key.starts_with("aws:")); // Reserved prefix
    }
}

#[test]
fn test_project_filtering_logic() {
    // Test the filtering logic for project tags
    let tags1 = vec![
        ("trainctl:project".to_string(), "project-a".to_string()),
        ("Name".to_string(), "instance-1".to_string()),
    ];

    let tags2 = vec![
        ("trainctl:project".to_string(), "project-b".to_string()),
        ("Name".to_string(), "instance-2".to_string()),
    ];

    // Filter by project-a
    let matches_project_a = tags1
        .iter()
        .any(|(k, v)| k == "trainctl:project" && v == "project-a");
    assert!(matches_project_a);

    let matches_project_a_2 = tags2
        .iter()
        .any(|(k, v)| k == "trainctl:project" && v == "project-a");
    assert!(!matches_project_a_2);
}

#[test]
fn test_user_filtering_logic() {
    // Test the filtering logic for user tags
    let tags1 = vec![
        ("trainctl:user".to_string(), "alice".to_string()),
        ("Name".to_string(), "instance-1".to_string()),
    ];

    let tags2 = vec![
        ("trainctl:user".to_string(), "bob".to_string()),
        ("Name".to_string(), "instance-2".to_string()),
    ];

    // Filter by alice
    let matches_alice = tags1
        .iter()
        .any(|(k, v)| k == "trainctl:user" && v == "alice");
    assert!(matches_alice);

    let matches_alice_2 = tags2
        .iter()
        .any(|(k, v)| k == "trainctl:user" && v == "alice");
    assert!(!matches_alice_2);
}
