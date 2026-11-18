//! Module 13 Adversarial Stress & State Machine Tests: Payments
//! Rigorously challenges invoice state machine under conflicting callbacks, double-charge attempts,
//! network errors, currency precision overflow, and reaction boundary clamping.

use gpui_telegram_proto::payments::payments_checkout_process::*;
use gpui_telegram_proto::payments::payments_form::*;
use gpui_telegram_proto::payments::payments_non_panel_process as non_panel;
use gpui_telegram_proto::payments::payments_reaction_process::*;
use gpui_telegram_proto::payments::ui::payments_form_summary::*;
use gpui_telegram_proto::payments::ui::payments_panel_data::*;

#[test]
fn test_adversarial_payments_checkout_conflicting_callbacks_and_double_charge() {
    let mut checkout = PaymentsCheckoutProcess::with_invoice("inv_adv_001", Mode::Payment);
    assert!(checkout.is_active());

    // 1. Payment start -> Validating
    assert!(checkout.register_payment_start().is_ok());
    assert_eq!(checkout.submit_state, SubmitState::Validating);

    // 2. Sudden network drop / error during validation
    let net_err = Error {
        code: 504,
        description: "GatewayTimeout".into(),
        message: "Upstream payment provider unreachable".into(),
        parameter: None,
    };
    assert!(checkout.handle_error(net_err).is_ok());
    assert_eq!(checkout.result, Some(CheckoutResult::Failed));
    assert_eq!(checkout.submit_state, SubmitState::None);
    assert!(!checkout.is_active());

    // 3. Recovery / re-start after error
    assert!(checkout.start().is_ok());
    assert!(checkout.is_active());
    assert_eq!(checkout.submit_state, SubmitState::None);

    // 4. Double-charge attempt: invoice_paid called multiple times
    let paid1 = checkout.invoice_paid();
    assert_eq!(paid1, Ok(CheckoutResult::Paid));
    assert_eq!(checkout.submit_state, SubmitState::Finishing);
    assert!(!checkout.is_active());

    // Second charge call on already paid invoice must remain Paid / Finishing without crash
    let paid2 = checkout.invoice_paid();
    assert_eq!(paid2, Ok(CheckoutResult::Paid));
    assert_eq!(checkout.result, Some(CheckoutResult::Paid));

    // 5. Unregister payment start resets state
    assert!(checkout.unregister_payment_start().is_ok());
    assert_eq!(checkout.submit_state, SubmitState::None);

    // 6. Close and reactivate with pending result
    checkout.close_and_reactivate(CheckoutResult::Pending);
    assert_eq!(checkout.result, Some(CheckoutResult::Pending));
    assert!(!checkout.is_enabled);
}

#[test]
fn test_adversarial_payments_form_invalid_inputs_and_credentials() {
    let mut form = PaymentsForm::new();

    // 1. Submit on uninitialized form without URL must fail validation
    assert!(!form.validate_information());
    let submit_res = form.submit();
    assert!(submit_res.is_err());
    assert_eq!(submit_res.unwrap_err(), "Form details invalid");

    // 2. Validate card with empty credentials
    assert!(!form.validate_card());

    // With empty new_credentials data
    form.payment_method.new_credentials = Some(NewCredentials {
        title: "Test".into(),
        data: Vec::new(),
        save_on_server: false,
    });
    assert!(!form.validate_card(), "Empty credentials data must fail validation");

    // With valid new_credentials
    form.payment_method.new_credentials = Some(NewCredentials {
        title: "Valid Card".into(),
        data: vec![1, 2, 3],
        save_on_server: true,
    });
    assert!(form.validate_card());

    // 3. Out-of-bounds saved_credentials index
    form.payment_method.new_credentials = None;
    form.payment_method.saved_credentials = vec![SavedCredentials {
        id: "cred_1".into(),
        title: "Visa 4242".into(),
    }];
    form.payment_method.saved_credentials_index = 5; // out of range
    assert!(!form.validate_card(), "Out of bounds saved credentials index must fail");

    form.payment_method.saved_credentials_index = 0;
    assert!(form.validate_card());

    // 4. Fill URL and verify successful submit
    form.details.url = "https://t.me/invoice_adv_99".into();
    assert!(form.validate_information());
    assert!(form.submit().is_ok());
    assert!(form.is_submitting);
}


#[test]
fn test_adversarial_form_summary_overflow_and_currency_precision() {
    let mut invoice = Invoice::default();
    invoice.currency = "USD".into();
    // 1. Extreme integer values: saturating_add check
    invoice.prices = vec![
        LabeledPrice { label: "Item 1".into(), price: i64::MAX / 3 },
        LabeledPrice { label: "Item 2".into(), price: i64::MAX / 3 },
        LabeledPrice { label: "Item 3".into(), price: i64::MAX / 3 },
    ];
    invoice.tips_max = i64::MAX;

    let mut summary = PaymentsFormSummaryUi::new(invoice);
    summary.selected_tip = 1_000_000;
    // Must not panic on integer overflow due to saturating_add
    let total = summary.compute_total_amount();
    assert!(total > 0);

    // 2. Currency formatting precision edge cases
    assert_eq!(PaymentsFormSummaryUi::format_amount(0, "USD"), "0.00 USD");
    assert_eq!(PaymentsFormSummaryUi::format_amount(1, "USD"), "0.01 USD");
    assert_eq!(PaymentsFormSummaryUi::format_amount(99, "EUR"), "0.99 EUR");
    assert_eq!(PaymentsFormSummaryUi::format_amount(100, "GBP"), "1.00 GBP");
    assert_eq!(PaymentsFormSummaryUi::format_amount(105, "JPY"), "1.05 JPY");
    assert_eq!(PaymentsFormSummaryUi::format_amount(-150, "USD"), "-1.50 USD");

    // 3. Suggested tips when items_sum is 0 or negative
    let mut zero_invoice = Invoice::default();
    zero_invoice.prices = vec![LabeledPrice { label: "Zero".into(), price: 0 }];
    let zero_summary = PaymentsFormSummaryUi::new(zero_invoice);
    assert!(zero_summary.suggested_tips_amounts().is_empty());

    // 4. Selecting tip exceeding tips_max
    let mut bounded_invoice = Invoice::default();
    bounded_invoice.tips_max = 500;
    let mut bounded_summary = PaymentsFormSummaryUi::new(bounded_invoice);
    bounded_summary.select_tip(1000); // Exceeds tips_max
    assert_eq!(bounded_summary.selected_tip, 0, "Tip exceeding tips_max must be rejected");

    bounded_summary.select_tip(300); // Within tips_max
    assert_eq!(bounded_summary.selected_tip, 300);
}

#[test]
fn test_adversarial_non_panel_and_reaction_boundary_clamping() {
    // 1. Non-panel process rejects fiat currency (only credits XTR / STARS allowed)
    let mut non_panel_proc = non_panel::PaymentsNonPanelProcess::new();
    let fiat_form = non_panel::CreditsFormData {
        form_id: 1,
        bot_id: 10,
        title: "Invalid Fiat".into(),
        description: "Fiat currency".into(),
        amount: 100,
        currency: "EUR".into(),
        star_gift_limited_count: None,
        star_gift_per_user_limit: None,
    };
    let fiat_err = non_panel_proc.process_credits_payment(fiat_form);
    assert!(fiat_err.is_err());
    assert_eq!(fiat_err.unwrap_err(), "Not a credits invoice");

    // 2. StarGift sold out handling
    non_panel_proc.handle_stargift_error("STARGIFT_USAGE_LIMITED", Some(10));
    assert_eq!(non_panel_proc.result, Some(non_panel::CheckoutResult::Failed));
    assert!(non_panel_proc.error_toast.as_ref().unwrap().contains("sold out (limit: 10)"));

    // 3. Reaction process clamping
    let mut react = PaymentsReactionProcess::for_message(100, 200);

    // Negative count clamped to 1
    react.set_reaction_count(-100);
    assert_eq!(react.selected_count, 1);

    // Massive count clamped to K_MAX_PER_REACTION_FALLBACK (10,000)
    react.set_reaction_count(500_000);
    assert_eq!(react.selected_count, K_MAX_PER_REACTION_FALLBACK);

    // 4. Donor list sorted descending by count
    react.add_donor(PaidReactionDonor { peer_id: 1, count: 50, is_anonymous: false, is_my: false });
    react.add_donor(PaidReactionDonor { peer_id: 2, count: 500, is_anonymous: true, is_my: true });
    react.add_donor(PaidReactionDonor { peer_id: 3, count: 200, is_anonymous: false, is_my: false });

    assert_eq!(react.top_donors[0].count, 500);
    assert_eq!(react.top_donors[1].count, 200);
    assert_eq!(react.top_donors[2].count, 50);
    assert!(react.my_paid_anonymous);
    assert!(react.lookup_my_paid_anonymous());
}
