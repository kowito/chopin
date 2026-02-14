use chopin::auth::password::{hash_password, verify_password};

#[test]
fn test_hash_and_verify_password() {
    let password = "secure_password_123";
    let hash = hash_password(password).expect("Failed to hash password");

    assert!(!hash.is_empty());
    assert_ne!(hash, password);

    let is_valid = verify_password(password, &hash).expect("Failed to verify password");
    assert!(is_valid);
}

#[test]
fn test_wrong_password_fails() {
    let correct_password = "correct123";
    let wrong_password = "wrong456";

    let hash = hash_password(correct_password).expect("Failed to hash");

    let is_valid = verify_password(wrong_password, &hash).expect("Failed to verify");
    assert!(!is_valid);
}

#[test]
fn test_case_sensitive_passwords() {
    let password = "Password123";
    let hash = hash_password(password).expect("Failed to hash");

    // Correct case
    assert!(verify_password("Password123", &hash).expect("Failed to verify"));

    // Wrong case
    assert!(!verify_password("password123", &hash).expect("Failed to verify"));
    assert!(!verify_password("PASSWORD123", &hash).expect("Failed to verify"));
}

#[test]
fn test_empty_password() {
    let password = "";
    let hash = hash_password(password).expect("Failed to hash empty password");

    assert!(verify_password("", &hash).expect("Failed to verify"));
    assert!(!verify_password("not empty", &hash).expect("Failed to verify"));
}

#[test]
fn test_long_password() {
    let password = "a".repeat(1000);
    let hash = hash_password(&password).expect("Failed to hash long password");

    assert!(verify_password(&password, &hash).expect("Failed to verify"));
    assert!(!verify_password("wrong", &hash).expect("Failed to verify"));
}

#[test]
fn test_special_characters_in_password() {
    let passwords = vec![
        "pass!@#$%^&*()",
        "unicode_🔒🔑",
        "with spaces in it",
        "tab\there",
        "newline\nhere",
        r#"quotes"and'stuff"#,
        "\\backslashes\\",
    ];

    for password in passwords {
        let hash = hash_password(password).expect("Failed to hash");
        assert!(verify_password(password, &hash).expect("Failed to verify"));
    }
}

#[test]
fn test_hash_produces_different_results() {
    let password = "same_password";

    let hash1 = hash_password(password).expect("Failed to hash 1");
    let hash2 = hash_password(password).expect("Failed to hash 2");

    // Hashes should be different due to random salt
    assert_ne!(hash1, hash2);

    // But both should verify the same password
    assert!(verify_password(password, &hash1).expect("Failed to verify 1"));
    assert!(verify_password(password, &hash2).expect("Failed to verify 2"));
}

#[test]
fn test_invalid_hash_format_fails() {
    let password = "test123";
    let invalid_hashes = vec![
        "",
        "not a valid hash",
        "random_string_123",
        "$2b$10$invalid",
    ];

    for invalid_hash in invalid_hashes {
        let result = verify_password(password, invalid_hash);
        assert!(
            result.is_err(),
            "Should fail for invalid hash format: {}",
            invalid_hash
        );
    }
}

#[test]
fn test_hash_format_is_argon2() {
    let password = "test123";
    let hash = hash_password(password).expect("Failed to hash");

    // Argon2 hashes start with $argon2
    assert!(
        hash.starts_with("$argon2"),
        "Hash should be Argon2 format: {}",
        hash
    );
}

#[test]
fn test_slightly_different_passwords() {
    let password1 = "password123";
    let password2 = "password124";

    let hash1 = hash_password(password1).expect("Failed to hash");

    assert!(verify_password(password1, &hash1).expect("Failed to verify correct"));
    assert!(!verify_password(password2, &hash1).expect("Failed to verify wrong"));
}

#[test]
fn test_password_with_null_bytes() {
    let password = "pass\0word";
    let hash = hash_password(password).expect("Failed to hash");

    assert!(verify_password(password, &hash).expect("Failed to verify"));
    assert!(!verify_password("password", &hash).expect("Failed to verify without null"));
}

#[test]
fn test_common_passwords() {
    let common_passwords = vec!["password", "123456", "qwerty", "abc123", "letmein", "admin"];

    for password in common_passwords {
        let hash = hash_password(password).expect("Failed to hash");
        assert!(verify_password(password, &hash).expect("Failed to verify"));
    }
}

#[test]
fn test_hash_is_not_reversible() {
    let password = "secure_password";
    let hash = hash_password(password).expect("Failed to hash");

    // Hash should not contain the original password
    assert!(!hash.contains(password));

    // Trying to use hash as password should fail
    let wrong = verify_password(&hash, &hash).expect("Failed to verify");
    assert!(!wrong);
}

#[test]
fn test_multiple_verifications_same_hash() {
    let password = "test123";
    let hash = hash_password(password).expect("Failed to hash");

    // Verify multiple times - should all succeed
    for _ in 0..10 {
        assert!(verify_password(password, &hash).expect("Failed to verify"));
    }
}

#[test]
fn test_concurrent_hashing() {
    use std::sync::Arc;
    use std::thread;

    let password = Arc::new("concurrent_test".to_string());
    let mut handles = vec![];

    for _ in 0..5 {
        let pwd = Arc::clone(&password);
        let handle = thread::spawn(move || {
            let hash = hash_password(&pwd).expect("Failed to hash");
            let is_valid = verify_password(&pwd, &hash).expect("Failed to verify");
            assert!(is_valid);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }
}
