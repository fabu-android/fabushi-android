#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebAuthnSignerCapability {
    Unavailable,
    AndroidCredentialManager,
    RemoteDesktopSigner,
}

pub fn webauthn_signer_ships(capability: WebAuthnSignerCapability) -> bool {
    !matches!(capability, WebAuthnSignerCapability::Unavailable)
}

pub fn webauthn_proxy_mirrored_enablement(
    enabled: bool,
    capability: WebAuthnSignerCapability,
) -> bool {
    enabled && webauthn_signer_ships(capability)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_native_signer_is_a_supported_platform_adaptation() {
        assert!(webauthn_proxy_mirrored_enablement(
            true,
            WebAuthnSignerCapability::AndroidCredentialManager
        ));
        assert!(!webauthn_proxy_mirrored_enablement(
            true,
            WebAuthnSignerCapability::Unavailable
        ));
        assert!(!webauthn_proxy_mirrored_enablement(
            false,
            WebAuthnSignerCapability::RemoteDesktopSigner
        ));
    }
}
