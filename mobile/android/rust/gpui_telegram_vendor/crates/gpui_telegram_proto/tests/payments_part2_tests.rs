use gpui_telegram_proto::payments::stripe::stripe_card_validator::*;
use gpui_telegram_proto::payments::stripe::stripe_decode::*;
use gpui_telegram_proto::payments::stripe::stripe_error::*;
use gpui_telegram_proto::payments::stripe::stripe_form_encodable::*;
use gpui_telegram_proto::payments::stripe::stripe_form_encoder::*;
use gpui_telegram_proto::payments::ui::payments_form_summary::*;
use gpui_telegram_proto::payments::ui::payments_panel_data::*;
use gpui_telegram_proto::payments::ui::payments_reaction_box::*;

#[test]
fn test_luhn_algorithm_and_brand_detection() {
    // Standard Visa test card (valid Luhn)
    let visa_valid = "4242 4242 4242 4242";
    let res = validate_card(visa_valid);
    assert_eq!(res.state, ValidationState::Valid);
    assert_eq!(res.brand, CardBrand::Visa);
    assert!(res.finished);

    // Amex test card prefix 37
    let amex_valid = "378282246310005";
    let res_amex = validate_card(amex_valid);
    assert_eq!(res_amex.brand, CardBrand::Amex);

    // Invalid Luhn checksum
    let invalid_card = "4242 4242 4242 4241";
    let res_inv = validate_card(invalid_card);
    assert_eq!(res_inv.state, ValidationState::Invalid);

    // Formats
    assert_eq!(card_number_format(amex_valid), vec![4, 6, 5]);
    assert_eq!(card_number_format(visa_valid), vec![4, 4, 4, 4]);
}

#[test]
fn test_expiry_and_cvc_validation() {
    let valid_exp = validate_expire_date("12/29", None);
    assert_eq!(valid_exp.state, ValidationState::Valid);
    assert!(valid_exp.finished);

    let expired = validate_expire_date("01/20", None);
    assert_eq!(expired.state, ValidationState::Invalid);

    let invalid_month = validate_expire_date("13/30", None);
    assert_eq!(invalid_month.state, ValidationState::Invalid);

    // CVC
    let visa_cvc = validate_cvc("4242424242424242", "123");
    assert_eq!(visa_cvc.state, ValidationState::Valid);
    assert!(visa_cvc.finished);

    let amex_cvc = validate_cvc("378282246310005", "1234");
    assert_eq!(amex_cvc.state, ValidationState::Valid);
    assert!(amex_cvc.finished);

    let amex_3digit = validate_cvc("378282246310005", "123");
    assert_eq!(amex_3digit.state, ValidationState::Valid);
    assert!(!amex_3digit.finished);

    let amex_short_cvc = validate_cvc("378282246310005", "12");
    assert_eq!(amex_short_cvc.state, ValidationState::Incomplete);
}

#[test]
fn test_stripe_form_encoder() {
    let mut encodable = MakeEncodable::new("card");
    encodable.insert("number", "4242424242424242");
    encodable.insert("cvc", "123");
    encodable.insert("exp_month", "12");

    let encoded_bytes = FormEncoder::form_encoded_data_for_object(&encodable);
    let encoded_str = String::from_utf8(encoded_bytes).unwrap();
    // Keys sorted: cvc, exp_month, number
    assert_eq!(
        encoded_str,
        "card[cvc]=123&card[exp_month]=12&card[number]=4242424242424242"
    );
}

#[test]
fn test_stripe_decode_and_error_parsing() {
    let json_error = serde_json::json!({
        "error": {
            "type": "card_error",
            "code": "incorrect_number",
            "message": "Your card number is incorrect.",
            "param": "number"
        }
    });

    let err = Error::decoded_object_from_response(&json_error);
    assert_eq!(err.code, Code::Card);
    assert_eq!(err.description, "IncorrectNumber");
    assert_eq!(err.message, "Your card number is incorrect.");
    assert_eq!(err.parameter, "number");

    let json_valid = serde_json::json!({
        "id": "tok_123",
        "livemode": false
    });
    assert!(contains_fields(&json_valid, &["id", "livemode"]));
    assert!(!contains_fields(&json_valid, &["id", "nonexistent"]));
}

#[test]
fn test_checkout_summary_and_tips() {
    let invoice = Invoice {
        prices: vec![
            LabeledPrice { label: "Item 1".into(), price: 1500 },
            LabeledPrice { label: "Item 2".into(), price: 500 },
        ],
        currency: "usd".into(),
        tips_max: 1000,
        tips_selected: 200,
        ..Default::default()
    };

    let mut summary = PaymentsFormSummaryUi::new(invoice);
    assert_eq!(summary.compute_total_amount(), 2200);
    assert_eq!(summary.formatted_total(), "22.00 USD");

    summary.select_tip(500);
    assert_eq!(summary.compute_total_amount(), 2500);
}

#[test]
fn test_payments_reaction_box_discreter_and_ranking() {
    assert_eq!(StarSelectDiscreter::value_to_index(1), 0);
    assert_eq!(StarSelectDiscreter::value_to_index(50), 4);
    assert_eq!(StarSelectDiscreter::index_to_value(4), 50);
    assert_eq!(format_count_decimal(10000), "10,000");

    let args = PaidReactionBoxArgs {
        current_balance: 500,
        default_stars: 50,
        top_reactors: vec![
            PaidReactionTop { name: "Alice".into(), stars_count: 100, ..Default::default() },
            PaidReactionTop { name: "Bob".into(), stars_count: 250, ..Default::default() },
        ],
        ..Default::default()
    };

    let mut box_ui = PaymentsReactionBoxUi::new(args);
    let ranked = box_ui.top_reactors_sorted();
    assert_eq!(ranked[0].name, "Bob");
    assert_eq!(ranked[0].rank, 1);
    assert_eq!(ranked[1].name, "Alice");
    assert_eq!(ranked[1].rank, 2);

    assert!(box_ui.send().is_ok());
    box_ui.set_stars(1000); // Exceeds balance of 500
    assert!(box_ui.send().is_err());
}
