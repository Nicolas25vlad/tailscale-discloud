pub fn should_start_login(_have_node_key: bool) -> bool {
    unimplemented!("implemented after the tests prove the intended login behavior")
}

#[cfg(test)]
mod tests {
    use super::should_start_login;

    #[test]
    fn starts_login_when_node_key_is_missing() {
        assert!(should_start_login(false));
    }

    #[test]
    fn skips_login_when_node_key_already_exists() {
        assert!(!should_start_login(true));
    }
}
