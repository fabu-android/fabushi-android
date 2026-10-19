use std::{
    fs,
    io,
    path::Path,
    process::{Command, Stdio},
};

pub const SAND_WEBAUTHN_PROXY_MARKER_PATH: &str = "/home/box/.sand-webauthn-proxy-enabled";
pub const BOX_CHROME_POLICY_COMMAND: &str = "/usr/local/bin/box-chrome-policy";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebAuthnProxyMarkerOutcome {
    NotABox,
    Unchanged,
    Applied,
}

pub fn apply_webauthn_proxy_marker(
    enabled: bool,
    marker_path: &Path,
    policy_command: &Path,
) -> io::Result<WebAuthnProxyMarkerOutcome> {
    let Some(parent) = marker_path.parent() else {
        return Ok(WebAuthnProxyMarkerOutcome::NotABox);
    };
    if !parent.exists() {
        return Ok(WebAuthnProxyMarkerOutcome::NotABox);
    }

    let present = marker_path.exists();
    if present == enabled {
        return Ok(WebAuthnProxyMarkerOutcome::Unchanged);
    }

    if enabled {
        fs::write(marker_path, b"")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(marker_path, fs::Permissions::from_mode(0o644))?;
        }
    } else {
        fs::remove_file(marker_path)?;
    }

    if policy_command.exists() {
        let _ = Command::new(policy_command)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    Ok(WebAuthnProxyMarkerOutcome::Applied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("fabushi-webauthn-marker-{}-{stamp}", std::process::id()))
    }

    #[test]
    fn marker_is_idempotent_and_box_directory_gated() {
        let root = temp_root();
        let marker = root.join("box/.sand-webauthn-proxy-enabled");
        let command = root.join("missing-policy");

        assert_eq!(
            apply_webauthn_proxy_marker(true, &marker, &command).unwrap(),
            WebAuthnProxyMarkerOutcome::NotABox
        );

        fs::create_dir_all(marker.parent().unwrap()).unwrap();
        assert_eq!(
            apply_webauthn_proxy_marker(true, &marker, &command).unwrap(),
            WebAuthnProxyMarkerOutcome::Applied
        );
        assert!(marker.exists());
        assert_eq!(
            apply_webauthn_proxy_marker(true, &marker, &command).unwrap(),
            WebAuthnProxyMarkerOutcome::Unchanged
        );
        assert_eq!(
            apply_webauthn_proxy_marker(false, &marker, &command).unwrap(),
            WebAuthnProxyMarkerOutcome::Applied
        );
        assert!(!marker.exists());

        let _ = fs::remove_dir_all(root);
    }
}
