#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenerConnectCard {
    pub message_type: &'static str,
    pub platform: String,
    pub reason: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ListenerConnectSurface {
    pub cards: Vec<ListenerConnectCard>,
    pub reminder: Option<String>,
}

pub fn surface_listener_connect_cards<Connected, Display>(
    platforms: &[String],
    mut is_connected: Option<Connected>,
    mut display_name: Display,
) -> ListenerConnectSurface
where
    Connected: FnMut(&str) -> Result<bool, String>,
    Display: FnMut(&str) -> Option<String>,
{
    let Some(ref mut is_connected) = is_connected else {
        return ListenerConnectSurface::default();
    };
    if platforms.is_empty() {
        return ListenerConnectSurface::default();
    }
    let disconnected = platforms
        .iter()
        .filter_map(|platform| match is_connected(platform) {
            Ok(true) => None,
            Ok(false) => Some(platform.clone()),
            Err(_) => None,
        })
        .collect::<Vec<_>>();
    if disconnected.is_empty() {
        return ListenerConnectSurface::default();
    }
    let cards = disconnected
        .iter()
        .cloned()
        .map(|platform| ListenerConnectCard {
            message_type: "listener-connect",
            platform,
            reason: "so this routine can fire",
        })
        .collect::<Vec<_>>();
    let names = disconnected
        .iter()
        .map(|platform| display_name(platform).unwrap_or_else(|| platform.clone()))
        .collect::<Vec<_>>()
        .join(" and ");
    let verb = if disconnected.len() == 1 { "isn't" } else { "aren't" };
    ListenerConnectSurface {
        cards,
        reminder: Some(format!("{names} {verb} connected to the user's Cursor account yet, so this routine won't fire until they connect. The connect card is already in the chat — say so in your own words, but don't paste a link or send them to settings, and don't ask them to report back: you're resumed automatically once it connects.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listener_cards_fail_soft_and_preserve_desktop_instruction() {
        let platforms = vec!["slack".to_string(), "github".to_string(), "linear".to_string()];
        let surfaced = surface_listener_connect_cards(
            &platforms,
            Some(|platform: &str| -> Result<bool, String> {
                match platform {
                    "slack" => Ok(false),
                    "github" => Ok(true),
                    _ => Err("lookup failed".into()),
                }
            }),
            |platform| (platform == "slack").then(|| "Slack".to_string()),
        );
        assert_eq!(surfaced.cards.len(), 1);
        assert_eq!(surfaced.cards[0].platform, "slack");
        assert_eq!(surfaced.cards[0].reason, "so this routine can fire");
        let reminder = surfaced.reminder.expect("reminder");
        assert!(reminder.starts_with("Slack isn't connected"));
        assert!(reminder.contains("don't paste a link"));
        assert!(reminder.contains("resumed automatically"));
    }
}
