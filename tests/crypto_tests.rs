use termii::services::crypto::{
    create_master_password, unlock_master_password, validate_master_password_policy,
};

#[test]
fn unlock_rejects_excessive_argon2_memory() {
    let (mut cfg, _key) = create_master_password("Abcdef!1").expect("create master password");
    cfg.argon2_memory_kib = (256 * 1024) + 1;
    let result = unlock_master_password(&cfg, "Abcdef!1");
    assert!(result.is_err());
}

#[test]
fn unlock_rejects_excessive_argon2_iterations() {
    let (mut cfg, _key) = create_master_password("Abcdef!1").expect("create master password");
    cfg.argon2_iterations = 11;
    let result = unlock_master_password(&cfg, "Abcdef!1");
    assert!(result.is_err());
}

#[test]
fn unlock_rejects_excessive_argon2_parallelism() {
    let (mut cfg, _key) = create_master_password("Abcdef!1").expect("create master password");
    cfg.argon2_parallelism = 5;
    let result = unlock_master_password(&cfg, "Abcdef!1");
    assert!(result.is_err());
}

#[test]
fn password_policy_rejects_space_as_only_symbol() {
    let result = validate_master_password_policy("Abcdef1 ");
    assert!(result.is_err());
}
