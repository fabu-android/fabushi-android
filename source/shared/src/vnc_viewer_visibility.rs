pub const VNC_VIEWER_VISIBLE_CHANNEL: &str = "sand:vnc-viewer-visible";

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channel_is_stable() { assert_eq!(VNC_VIEWER_VISIBLE_CHANNEL, "sand:vnc-viewer-visible"); }
}
