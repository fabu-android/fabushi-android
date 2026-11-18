//! Comprehensive Test Suite for Module 13 Payments Part 1
use gpui_telegram_proto::payments::payments_checkout_process as checkout_proc;
use gpui_telegram_proto::payments::payments_form as form_mod;
use gpui_telegram_proto::payments::payments_non_panel_process as non_panel;
use gpui_telegram_proto::payments::payments_reaction_process as react_proc;
use gpui_telegram_proto::payments::smartglocal::smartglocal_api_client as sg_client;
use gpui_telegram_proto::payments::smartglocal::smartglocal_card as sg_card;
use gpui_telegram_proto::payments::stripe::stripe_address as stripe_addr;
use gpui_telegram_proto::payments::stripe::stripe_card as stripe_c;
use gpui_telegram_proto::payments::stripe::stripe_card_params as stripe_params;

#[test]
fn test_payments_checkout_process_state_machine() {
    let mut checkout = checkout_proc::PaymentsCheckoutProcess::with_invoice("inv_test_123", checkout_proc::Mode::Payment);
    assert!(checkout.is_active());
    assert_eq!(checkout.submit_state, checkout_proc::SubmitState::None);

    checkout.register_payment_start().unwrap();
    assert_eq!(checkout.submit_state, checkout_proc::SubmitState::Validating);

    let update = checkout_proc::FormUpdate {
        form_id: 42,
        total_amount: 1500,
        currency: "USD".to_string(),
        shipping_options: vec!["express".to_string(), "standard".to_string()],
    };
    checkout.handle_form_update(update).unwrap();
    assert_eq!(checkout.submit_state, checkout_proc::SubmitState::Validated);

    let result = checkout.invoice_paid().unwrap();
    assert_eq!(result, checkout_proc::CheckoutResult::Paid);
    assert_eq!(checkout.submit_state, checkout_proc::SubmitState::Finishing);
    assert!(!checkout.is_active());
}

#[test]
fn test_payments_form_validation_and_tips() {
    let mut form = form_mod::PaymentsForm::new();
    assert!(!form.validate_information());

    form.details.form_id = 1001;
    form.details.url = "https://t.me/$invoice_abc".to_string();
    assert!(form.validate_information());

    form.set_tips(250);
    assert_eq!(form.tip_amount, 250);

    form.set_shipping_option("dhl_express");
    assert_eq!(form.selected_shipping_id.as_deref(), Some("dhl_express"));

    form.accept_terms();
    assert!(form.details.terms_accepted);

    assert!(form.submit().is_ok());
    assert!(form.is_submitting);
}

#[test]
fn test_payments_non_panel_credits_process() {
    let mut process = non_panel::PaymentsNonPanelProcess::new();
    assert!(non_panel::is_credits_invoice("XTR"));
    assert!(non_panel::is_credits_invoice("STARS"));
    assert!(!non_panel::is_credits_invoice("USD"));

    let credits_form = non_panel::CreditsFormData {
        form_id: 777,
        bot_id: 100,
        title: "Telegram Stars Gift".to_string(),
        description: "Special digital pack".to_string(),
        amount: 50,
        currency: "XTR".to_string(),
        star_gift_limited_count: Some(500),
        star_gift_per_user_limit: Some(5),
    };

    assert!(process.process_credits_payment(credits_form).is_ok());
    let outcome = process.handle_small_balance_result(non_panel::SmallBalanceResult::Success);
    assert_eq!(outcome, Some(non_panel::CheckoutResult::Paid));
    assert!(process.show_fireworks);

    // Test sold-out error handling
    let mut sold_out_proc = non_panel::PaymentsNonPanelProcess::new();
    sold_out_proc.handle_stargift_error("STARGIFT_USAGE_LIMITED", Some(100));
    assert_eq!(sold_out_proc.result, Some(non_panel::CheckoutResult::Failed));
    assert!(sold_out_proc.error_toast.unwrap().contains("sold out"));
}

#[test]
fn test_payments_reaction_process_anonymous_and_donors() {
    let mut react = react_proc::PaymentsReactionProcess::for_message(100200300, 42);
    react.set_reaction_count(150);
    assert_eq!(react.selected_count, 150);

    // Clamp check
    react.set_reaction_count(50_000);
    assert_eq!(react.selected_count, react_proc::K_MAX_PER_REACTION_FALLBACK);

    let donor_1 = react_proc::PaidReactionDonor {
        peer_id: 111,
        count: 200,
        is_anonymous: false,
        is_my: false,
    };
    let donor_2 = react_proc::PaidReactionDonor {
        peer_id: 222,
        count: 500,
        is_anonymous: true,
        is_my: true,
    };
    react.add_donor(donor_1);
    react.add_donor(donor_2);

    assert_eq!(react.top_donors[0].count, 500); // Sorted descending
    assert!(react.lookup_my_paid_anonymous());
}

#[test]
fn test_smartglocal_url_computation_and_token() {
    let test_config = sg_client::PaymentConfiguration {
        public_token: "tok_test_abc".to_string(),
        tokenize_url: String::new(),
        is_test: true,
    };
    assert_eq!(
        sg_client::compute_api_url(&test_config),
        "https://tgb-playground.smart-glocal.com/cds/v1/tokenize/card"
    );

    let prod_config = sg_client::PaymentConfiguration {
        public_token: "tok_prod_123".to_string(),
        tokenize_url: String::new(),
        is_test: false,
    };
    assert_eq!(
        sg_client::compute_api_url(&prod_config),
        "https://tgb.smart-glocal.com/cds/v1/tokenize/card"
    );

    // Last4 extraction
    let card = sg_card::Card {
        card_type: "visa".to_string(),
        network: "visa".to_string(),
        masked_number: "411111******1234".to_string(),
    };
    assert_eq!(card.last4(), "1234");
    assert!(!card.is_empty());
}

#[test]
fn test_stripe_card_params_form_encoding() {
    let params = stripe_params::CardParams {
        number: "4242424242424242".to_string(),
        exp_month: 12,
        exp_year: 2028,
        cvc: "314".to_string(),
        name: "Jane Doe".to_string(),
        address_line1: "123 Main St".to_string(),
        address_line2: "".to_string(),
        address_city: "San Francisco".to_string(),
        address_state: "CA".to_string(),
        address_zip: "94105".to_string(),
        address_country: "US".to_string(),
        currency: "USD".to_string(),
    };

    let encoded = params.to_form_encoded();
    assert!(encoded.contains("card[number]=4242424242424242"));
    assert!(encoded.contains("card[cvc]=314"));
    assert!(encoded.contains("card[exp_month]=12"));
    assert!(encoded.contains("card[exp_year]=2028"));
    assert!(encoded.contains("card[name]=Jane%20Doe"));
    assert!(encoded.contains("card[address_city]=San%20Francisco"));
}

#[test]
fn test_stripe_card_brand_parsing() {
    assert_eq!(stripe_c::brand_from_string("visa"), stripe_c::CardBrand::Visa);
    assert_eq!(stripe_c::brand_from_string("american express"), stripe_c::CardBrand::Amex);
    assert_eq!(stripe_c::brand_from_string("mastercard"), stripe_c::CardBrand::MasterCard);
    assert_eq!(stripe_c::brand_from_string("discover"), stripe_c::CardBrand::Discover);
    assert_eq!(stripe_c::brand_from_string("jcb"), stripe_c::CardBrand::JCB);
    assert_eq!(stripe_c::brand_from_string("diners club"), stripe_c::CardBrand::DinersClub);
    assert_eq!(stripe_c::brand_from_string("unknown_brand"), stripe_c::CardBrand::Unknown);

    assert_eq!(stripe_c::card_brand_to_string(stripe_c::CardBrand::Visa), "Visa");
    assert_eq!(stripe_c::card_brand_to_string(stripe_c::CardBrand::Amex), "American Express");
}

#[test]
fn test_stripe_address_requirements() {
    let addr = stripe_addr::StripeAddress {
        country: "US".to_string(),
        state: "NY".to_string(),
        city: "New York".to_string(),
        line1: "5th Ave".to_string(),
        line2: "".to_string(),
        postal_code: "10001".to_string(),
    };

    assert!(addr.is_valid_for(stripe_addr::BillingAddressFields::None));
    assert!(addr.is_valid_for(stripe_addr::BillingAddressFields::Zip));
    assert!(addr.is_valid_for(stripe_addr::BillingAddressFields::Full));

    let empty_zip = stripe_addr::StripeAddress {
        postal_code: "".to_string(),
        ..addr.clone()
    };
    assert!(!empty_zip.is_valid_for(stripe_addr::BillingAddressFields::Zip));
    assert!(!empty_zip.is_valid_for(stripe_addr::BillingAddressFields::Full));
}
