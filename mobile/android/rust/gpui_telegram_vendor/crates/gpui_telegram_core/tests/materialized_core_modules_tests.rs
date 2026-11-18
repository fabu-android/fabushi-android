use gpui_telegram_core::core::*;
use gpui_telegram_core::mtproto::*;

#[test]
fn test_core_submodules_instantiation_and_defaults() {
    let mut apiwrap = Apiwrap::new(1001);
    assert!(!apiwrap.is_connected);
    apiwrap.set_connected(true);
    assert!(apiwrap.is_connected);

    let mut app = Application::new();
    assert!(app.is_running());
    assert!(!app.has_active_window());
    app.register_window(gpui_telegram_core::core::WindowId(1), "TelegramGPUI".to_string(), true);
    assert!(app.has_active_window());

    let bank_card = BankCardClickHandler::new("4532123456789012");
    assert_eq!(bank_card.formatted_card_number(), "4532 1234 5678 9012");

    let config = Config::new();
    assert!(config.is_production());

    let changelogs = Changelogs::new(0);
    assert!(changelogs.has_unseen_updates(7003003));

    let cloud_password = CoreCloudPassword::new();
    assert!(!cloud_password.verify_state());

    let mut screenshot_prot = CoreScreenshotProtection::new();
    let r_id = screenshot_prot.add_content_reason(1);
    assert!(screenshot_prot.is_window_protected(1));
    screenshot_prot.remove_content_reason(1, r_id);
    assert!(!screenshot_prot.is_window_protected(1));
}

#[test]
fn test_mtproto_submodules_instantiation_and_defaults() {
    let config_loader = ConfigLoader::new();
    assert!(config_loader.is_active());
    assert_eq!(config_loader.label, "ConfigLoader");

    let conn_http = ConnectionHttp::new();
    assert!(conn_http.is_active());

    let conn_tcp = ConnectionTcp::new();
    assert!(conn_tcp.is_active());

    let auth_key = MtprotoAuthKey::new();
    assert!(auth_key.is_active());

    let session = Session::new();
    assert!(session.is_active());

    let proxy_data = MtprotoProxyData::new();
    assert!(proxy_data.is_active());

    let sender = Sender::new();
    assert!(sender.is_active());
}

#[test]
fn test_core_deep_links_instantiation() {
    use gpui_telegram_core::core::deep_links::*;

    let mut router = DeepLinksRouterDeepLinks::new();
    DeepLinksChatsDeepLinks::register(&mut router);
    DeepLinksContactsDeepLinks::register(&mut router);
    DeepLinksNewDeepLinks::register(&mut router);
    DeepLinksSettingsDeepLinks::register(&mut router);
    assert!(!router.routes.is_empty());
}
