//! Unit and Integration Tests for Module 13: Telegram Passport Subsystem
//! Covers: PassportEncryption, PassportEditIdentityBox, PassportFormController,
//! PassportFormRow, PassportFormViewController, PassportPanel, PassportPanelController,
//! PassportPanelEditContact, PassportPanelEditDocument, PassportPanelEditScans,
//! PassportPanelForm, PassportPanelPassword, PassportDetailsRowUi, PassportFormRowUi.

use std::collections::BTreeMap;
use gpui_telegram_core::passport::*;
use gpui_telegram_core::passport::passport_form_controller::{FileType, FormRequest, Value, ValueType};
use gpui_telegram_core::passport::passport_panel_controller::{ScopeErrorKey, ScopeErrorGeneral};

#[test]
fn test_passport_encryption_modulo_255_and_generation() {
    let secret = PassportEncryption::generate_secret_bytes();
    assert_eq!(secret.len(), 32);
    assert!(PassportEncryption::check_bytes_mod_255(&secret));
    assert!(PassportEncryption::check_secret_bytes(&secret));

    let secret_id = PassportEncryption::count_secure_secret_id(&secret);
    assert_ne!(secret_id, 0);

    let invalid = vec![1u8; 32];
    assert!(!PassportEncryption::check_bytes_mod_255(&invalid));
}

#[test]
fn test_passport_encryption_roundtrip() {
    let payload = b"Hello Telegram Passport Secret Encrypted Document!";
    let encrypted = PassportEncryption::encrypt_data(payload, None).expect("encryption succeeds");

    assert_eq!(encrypted.secret.len(), 32);
    assert_eq!(encrypted.hash.len(), 32);
    assert_eq!(encrypted.bytes.len() % 16, 0);

    let decrypted = PassportEncryption::decrypt_data(&encrypted.bytes, &encrypted.hash, &encrypted.secret)
        .expect("decryption succeeds");
    assert_eq!(decrypted, payload);
}

#[test]
fn test_passport_encryption_value_secret() {
    let master_secret = PassportEncryption::generate_secret_bytes();
    let value_secret = PassportEncryption::generate_secret_bytes();
    let value_hash = vec![42u8; 32];

    let encrypted = PassportEncryption::encrypt_value_secret(&value_secret, &master_secret, &value_hash)
        .expect("encrypt value secret");
    let decrypted = PassportEncryption::decrypt_value_secret(&encrypted, &master_secret, &value_hash)
        .expect("decrypt value secret");
    assert_eq!(decrypted, value_secret);
}

#[test]
fn test_passport_data_serialization() {
    let mut map = BTreeMap::new();
    map.insert("first_name".to_string(), "Pavel".to_string());
    map.insert("last_name".to_string(), "Durov".to_string());

    let bytes = PassportEncryption::serialize_data(&map).expect("serialize");
    let deserialized = PassportEncryption::deserialize_data(&bytes).expect("deserialize");
    assert_eq!(deserialized, map);
}

#[test]
fn test_passport_edit_identity_box() {
    let data = IdentityData {
        name: "Alice".to_string(),
        surname: "Smith".to_string(),
        middle_name: None,
    };
    let mut box_model = PassportEditIdentityBox::new(0, data, Vec::new());
    assert!(!box_model.has_changes);
    assert!(box_model.validate().is_err()); // No scans

    box_model.add_scan(ScanInfo {
        file_id: 101,
        file_name: "passport_scan.jpg".to_string(),
        file_size: 1024,
        mime_type: "image/jpeg".to_string(),
        thumbnail: Some(vec![1, 2, 3]),
        status: "ready".to_string(),
        is_deleted: false,
        error: None,
    });
    assert!(box_model.has_changes);
    assert_eq!(box_model.active_scans_count(), 1);
    assert!(box_model.validate().is_ok());

    box_model.delete_scan(101);
    assert_eq!(box_model.active_scans_count(), 0);
    assert!(box_model.validate().is_err());

    box_model.restore_scan(101);
    assert_eq!(box_model.active_scans_count(), 1);
    assert!(box_model.validate().is_ok());
}

#[test]
fn test_passport_form_controller_requirements() {
    let req = FormRequest {
        bot_id: 123456,
        scope: "id_docs".to_string(),
        callback_url: "https://example.com/callback".to_string(),
        public_key: "dummy_rsa".to_string(),
        nonce: "test_nonce".to_string(),
    };
    let mut controller = PassportFormController::new(req);
    controller.form.requested_groups = vec![
        vec![ValueType::PersonalDetails],
        vec![ValueType::Passport, ValueType::DriverLicense],
    ];

    assert!(!controller.can_submit());
    assert_eq!(controller.missing_requirements().len(), 2);

    let mut personal = Value::new(ValueType::PersonalDetails);
    personal.fields.insert("first_name".to_string(), "John".to_string());
    controller.add_value(personal);

    let mut passport_val = Value::new(ValueType::Passport);
    passport_val.fields.insert("number".to_string(), "A1234567".to_string());
    controller.add_value(passport_val);

    assert!(controller.can_submit());
    assert!(controller.missing_requirements().is_empty());
}

#[test]
fn test_passport_form_row_and_view_controller() {
    let mut row = PassportFormRow::new("identity", "Identity Document", "Passport or ID card");
    assert!(!row.is_ready());
    assert!(!row.has_error());

    row.set_ready(true);
    assert!(row.is_ready());
    assert_eq!(row.inner.error_message, None);

    row.set_error(Some("Scan rejected: blurry".to_string()));
    assert!(!row.is_ready());
    assert!(row.has_error());

    let req = FormRequest {
        bot_id: 1,
        scope: "test".to_string(),
        callback_url: "url".to_string(),
        public_key: "pk".to_string(),
        nonce: "n".to_string(),
    };
    let mut form_ctrl = PassportFormController::new(req);
    form_ctrl.form.requested_groups = vec![
        vec![ValueType::PersonalDetails],
        vec![ValueType::Phone],
        vec![ValueType::Email],
    ];

    let mut view_ctrl = PassportFormViewController::new();
    view_ctrl.refresh(&form_ctrl.form);
    assert_eq!(view_ctrl.scopes.len(), 3);
    assert_eq!(view_ctrl.rows.len(), 3);
}

#[test]
fn test_passport_panel_and_panel_controller() {
    let mut panel = PassportPanel::new();
    assert_eq!(panel.panel_width, 392.0);
    assert_eq!(panel.panel_height, 600.0);
    assert!(!panel.back_allowed);

    panel.show_edit_scope(1);
    assert!(panel.back_allowed);

    panel.push_toast("Scan uploaded successfully");
    assert_eq!(panel.pop_toast().unwrap(), "Scan uploaded successfully");
    assert_eq!(panel.close(), 200);

    let mut panel_ctrl = PassportPanelController::new();
    panel_ctrl.add_scope_error(
        ScopeErrorKey::General(ScopeErrorGeneral::ScanMissing),
        "Document scan required",
    );
    assert_eq!(panel_ctrl.scope_errors.len(), 1);

    // Cyrillic transliteration test
    let translit = PassportPanelController::native_to_latin("Иван Иванов");
    assert_eq!(translit, "Ivan Ivanov");
}

#[test]
fn test_passport_panel_edit_contact() {
    let mut phone_ctrl = PassportPanelEditContact::new_phone("+14155552671", None);
    assert!(phone_ctrl.validate().is_ok());

    let formatted = PassportPanelEditContact::format_phone_digits("14155552671");
    assert_eq!(formatted, "+1 (415) 555-2671");

    phone_ctrl.start_verification(6, 60);
    assert!(phone_ctrl.submit_code("123").is_err());
    assert!(phone_ctrl.submit_code("123456").is_ok());
    assert!(phone_ctrl.is_saved);

    let email_ctrl = PassportPanelEditContact::new_email("invalid-email", None);
    assert!(email_ctrl.validate().is_err());
}

#[test]
fn test_passport_panel_edit_document_and_scans() {
    let mut doc_ctrl = PassportPanelEditDocument::new(vec![
        passport_panel_edit_document::EditDocumentSchemeRow {
            value_class: passport_panel_edit_document::EditDocumentValueClass::Fields,
            key: "first_name".to_string(),
            label: "First Name".to_string(),
            length_limit: 50,
            key_for_attachment_to: None,
            additional_fallback_key: None,
        },
    ]);
    assert!(!doc_ctrl.validate());
    doc_ctrl.set_field("first_name", "Dmitry");
    assert!(doc_ctrl.validate());

    let mut scans_ctrl = PassportPanelEditScans::new("ID Scans");
    let scan = passport_panel_edit_scans::ScanItem {
        file_id: 1,
        file_type: FileType::Scan,
        title: "passport_main.jpg".to_string(),
        file_size: 500_000,
        status: "ready".to_string(),
        thumbnail: None,
        is_deleted: false,
        error: None,
    };
    assert!(scans_ctrl.add_scan(scan).is_ok());
    assert_eq!(scans_ctrl.active_scans_count(), 1);

    let oversized = passport_panel_edit_scans::ScanItem {
        file_id: 2,
        file_type: FileType::Scan,
        title: "huge.raw".to_string(),
        file_size: 15 * 1024 * 1024,
        status: "too_large".to_string(),
        thumbnail: None,
        is_deleted: false,
        error: None,
    };
    assert!(scans_ctrl.add_scan(oversized).is_err());
}

#[test]
fn test_passport_panel_form_and_password() {
    let mut form_view = PassportPanelForm::new("Telegram Wallet Bot", "wallet");
    assert!(!form_view.can_authorize);
    assert!(form_view.begin_authorization().is_err());

    let mut pwd = PassportPanelPassword::new(true);
    assert!(pwd.submit().is_err()); // empty
    pwd.set_password_input("SuperSecretPassword123!");
    assert_eq!(pwd.submit().unwrap(), "SuperSecretPassword123!");
    assert!(pwd.is_submitting);

    pwd.recover();
    assert!(pwd.recovery_requested);
}

#[test]
fn test_passport_ui_rows() {
    let mut details_row = PassportDetailsRowUi::new(PanelDetailsType::Date, "Expiry Date", 100.0);
    details_row.set_field_value("01012030");
    assert_eq!(details_row.field_value, "01.01.2030");
    assert!(details_row.calculate_height() > 30.0);

    let mut form_row_ui = PassportFormRowUi::new("Utility Bill", "Uploaded 1 bill");
    assert!(!form_row_ui.is_ready);
    form_row_ui.update_content("Utility Bill", "Accepted", true, false);
    assert!(form_row_ui.is_ready);
    assert!(!form_row_ui.is_error);
}
