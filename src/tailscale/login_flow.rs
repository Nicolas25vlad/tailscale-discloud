#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginMethod {
    None,
    AuthKey,
    Interactive,
}

pub fn choose_login_method(have_node_key: bool, has_auth_key: bool) -> LoginMethod {
    if have_node_key {
        LoginMethod::None
    } else if has_auth_key {
        LoginMethod::AuthKey
    } else {
        LoginMethod::Interactive
    }
}

#[cfg(test)]
mod tests {
    use super::{choose_login_method, LoginMethod};

    #[test]
    fn skips_login_when_node_key_already_exists() {
        assert_eq!(choose_login_method(true, true), LoginMethod::None);
        assert_eq!(choose_login_method(true, false), LoginMethod::None);
    }

    #[test]
    fn uses_auth_key_for_fresh_node_when_available() {
        assert_eq!(choose_login_method(false, true), LoginMethod::AuthKey);
    }

    #[test]
    fn falls_back_to_interactive_login_without_auth_key() {
        assert_eq!(choose_login_method(false, false), LoginMethod::Interactive);
    }
}
