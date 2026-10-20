#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CursorAccountStatus {
    pub kind: String,
    pub auth_id: Option<String>,
    pub email: Option<String>,
}

pub fn cursor_account_slot(status: &CursorAccountStatus) -> Option<String> {
    if status.kind != "logged-in" {
        return None;
    }
    status
        .auth_id
        .as_ref()
        .or(status.email.as_ref())
        .filter(|value| !value.is_empty())
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logged_in_prefers_auth_id_then_email() {
        assert_eq!(
            cursor_account_slot(&CursorAccountStatus {
                kind: "logged-in".into(),
                auth_id: Some("auth".into()),
                email: Some("mail@example.com".into()),
            })
            .as_deref(),
            Some("auth")
        );
        assert_eq!(
            cursor_account_slot(&CursorAccountStatus {
                kind: "logged-in".into(),
                auth_id: None,
                email: Some("mail@example.com".into()),
            })
            .as_deref(),
            Some("mail@example.com")
        );
        assert!(cursor_account_slot(&CursorAccountStatus {
            kind: "logged-out".into(),
            auth_id: Some("auth".into()),
            email: None,
        })
        .is_none());
    }
}
