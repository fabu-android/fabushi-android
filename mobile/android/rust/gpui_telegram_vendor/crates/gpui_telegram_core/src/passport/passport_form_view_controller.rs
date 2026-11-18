//! Telegram Passport Form View Controller & Scope Computation
//! Upstream: `Telegram/SourceFiles/passport/passport_form_view_controller.h`, `passport_form_view_controller.cpp`
//! Subsystem: `passport`, Feature ID: `TCG-N02`

use serde::{Deserialize, Serialize};
use super::passport_form_controller::{Form, ValueType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ScopeType {
    PersonalDetails,
    Identity,
    AddressDetails,
    Address,
    Phone,
    Email,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub scope_type: ScopeType,
    pub details: Option<ValueType>,
    pub documents: Vec<ValueType>,
}

impl Scope {
    pub fn new(scope_type: ScopeType) -> Self {
        Self {
            scope_type,
            details: None,
            documents: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeRow {
    pub title: String,
    pub description: String,
    pub ready_text: String,
    pub error_text: Option<String>,
    pub is_complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PassportFormViewController {
    pub scopes: Vec<Scope>,
    pub rows: Vec<ScopeRow>,
}

impl PassportFormViewController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn can_have_errors(val_type: ValueType) -> bool {
        match val_type {
            ValueType::PersonalDetails
            | ValueType::Passport
            | ValueType::DriverLicense
            | ValueType::IdentityCard
            | ValueType::InternalPassport
            | ValueType::Address
            | ValueType::UtilityBill
            | ValueType::BankStatement
            | ValueType::RentalAgreement
            | ValueType::PassportRegistration
            | ValueType::TemporaryRegistration => true,
            ValueType::Phone | ValueType::Email => false,
        }
    }

    pub fn compute_scopes(form: &Form) -> Vec<Scope> {
        let mut scopes = Vec::new();
        let mut has_personal = false;
        let mut has_identity = false;
        let mut has_address_details = false;
        let mut has_address = false;
        let mut has_phone = false;
        let mut has_email = false;

        for group in &form.requested_groups {
            for vt in group {
                match vt {
                    ValueType::PersonalDetails => has_personal = true,
                    ValueType::Passport | ValueType::DriverLicense | ValueType::IdentityCard | ValueType::InternalPassport => {
                        has_identity = true;
                    }
                    ValueType::Address => has_address_details = true,
                    ValueType::UtilityBill | ValueType::BankStatement | ValueType::RentalAgreement | ValueType::PassportRegistration | ValueType::TemporaryRegistration => {
                        has_address = true;
                    }
                    ValueType::Phone => has_phone = true,
                    ValueType::Email => has_email = true,
                }
            }
        }

        if has_personal {
            let mut s = Scope::new(ScopeType::PersonalDetails);
            s.details = Some(ValueType::PersonalDetails);
            scopes.push(s);
        }
        if has_identity {
            let mut s = Scope::new(ScopeType::Identity);
            s.documents = vec![
                ValueType::Passport,
                ValueType::DriverLicense,
                ValueType::IdentityCard,
                ValueType::InternalPassport,
            ];
            scopes.push(s);
        }
        if has_address_details {
            let mut s = Scope::new(ScopeType::AddressDetails);
            s.details = Some(ValueType::Address);
            scopes.push(s);
        }
        if has_address {
            let mut s = Scope::new(ScopeType::Address);
            s.documents = vec![
                ValueType::UtilityBill,
                ValueType::BankStatement,
                ValueType::RentalAgreement,
                ValueType::PassportRegistration,
                ValueType::TemporaryRegistration,
            ];
            scopes.push(s);
        }
        if has_phone {
            let mut s = Scope::new(ScopeType::Phone);
            s.details = Some(ValueType::Phone);
            scopes.push(s);
        }
        if has_email {
            let mut s = Scope::new(ScopeType::Email);
            s.details = Some(ValueType::Email);
            scopes.push(s);
        }
        scopes
    }

    pub fn compute_scope_row_ready_string(scope: &Scope, form: &Form) -> String {
        match scope.scope_type {
            ScopeType::PersonalDetails => {
                if let Some(v) = form.values.get(&ValueType::PersonalDetails) {
                    let first = v.fields.get("first_name").cloned().unwrap_or_default();
                    let last = v.fields.get("last_name").cloned().unwrap_or_default();
                    if !first.is_empty() && !last.is_empty() {
                        return format!("{first} {last}");
                    }
                }
                "Not provided".to_string()
            }
            ScopeType::Identity => {
                for vt in &scope.documents {
                    if let Some(v) = form.values.get(vt) {
                        if v.is_filled() {
                            return format!("{vt:?} uploaded");
                        }
                    }
                }
                "Document scan required".to_string()
            }
            ScopeType::AddressDetails => {
                if let Some(v) = form.values.get(&ValueType::Address) {
                    let street = v.fields.get("street_line1").cloned().unwrap_or_default();
                    let city = v.fields.get("city").cloned().unwrap_or_default();
                    if !city.is_empty() {
                        return format!("{street}, {city}");
                    }
                }
                "Not provided".to_string()
            }
            ScopeType::Address => {
                for vt in &scope.documents {
                    if let Some(v) = form.values.get(vt) {
                        if v.is_filled() {
                            return format!("{vt:?} uploaded");
                        }
                    }
                }
                "Proof of address required".to_string()
            }
            ScopeType::Phone => {
                if let Some(v) = form.values.get(&ValueType::Phone) {
                    if let Some(phone) = v.fields.get("phone") {
                        return phone.clone();
                    }
                }
                "Not verified".to_string()
            }
            ScopeType::Email => {
                if let Some(v) = form.values.get(&ValueType::Email) {
                    if let Some(email) = v.fields.get("email") {
                        return email.clone();
                    }
                }
                "Not verified".to_string()
            }
        }
    }

    pub fn compute_scope_row(scope: &Scope, form: &Form) -> ScopeRow {
        let (title, description) = match scope.scope_type {
            ScopeType::PersonalDetails => ("Personal Details", "Your legal name, birth date and gender"),
            ScopeType::Identity => ("Identity Document", "Passport, Driver's License or ID Card"),
            ScopeType::AddressDetails => ("Residential Address", "Your permanent living address"),
            ScopeType::Address => ("Proof of Address", "Utility bill, bank statement or rental agreement"),
            ScopeType::Phone => ("Phone Number", "Your verified mobile phone number"),
            ScopeType::Email => ("Email Address", "Your verified contact email address"),
        };

        let ready_text = Self::compute_scope_row_ready_string(scope, form);
        let is_complete = !ready_text.contains("required") && !ready_text.contains("Not ");

        ScopeRow {
            title: title.to_string(),
            description: description.to_string(),
            ready_text,
            error_text: None,
            is_complete,
        }
    }

    pub fn refresh(&mut self, form: &Form) {
        self.scopes = Self::compute_scopes(form);
        self.rows = self.scopes.iter().map(|s| Self::compute_scope_row(s, form)).collect();
    }
}
