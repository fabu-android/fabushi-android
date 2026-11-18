//! Telegram Passport Form Controller & MTProto Orchestration
//! Upstream: `Telegram/SourceFiles/passport/passport_form_controller.h`, `passport_form_controller.cpp`
//! Subsystem: `passport`, Feature ID: `TCG-N02`

use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

pub const NOTHING_FILLED: i32 = 0x100;
pub const NO_TRANSLATION_FILLED: i32 = 0x10;
pub const NO_SELFIE_FILLED: i32 = 0x001;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum FileType {
    Scan,
    Translation,
    FrontSide,
    ReverseSide,
    Selfie,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ValueType {
    PersonalDetails,
    Passport,
    DriverLicense,
    IdentityCard,
    InternalPassport,
    Address,
    UtilityBill,
    BankStatement,
    RentalAgreement,
    PassportRegistration,
    TemporaryRegistration,
    Phone,
    Email,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LoadStatus {
    #[default]
    Done,
    InProgress,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct File {
    pub id: u64,
    pub access_hash: u64,
    pub size: usize,
    pub dc_id: i32,
    pub date: i64,
    pub hash: Vec<u8>,
    pub secret: Vec<u8>,
    pub encrypted_secret: Vec<u8>,
    pub status: LoadStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Value {
    pub value_type: Option<ValueType>,
    pub fields: BTreeMap<String, String>,
    pub files: BTreeMap<FileType, Vec<File>>,
    pub selfie_required: bool,
    pub translation_required: bool,
    pub native_names: bool,
    pub error: Option<String>,
}

impl Value {
    pub fn new(value_type: ValueType) -> Self {
        Self {
            value_type: Some(value_type),
            fields: BTreeMap::new(),
            files: BTreeMap::new(),
            selfie_required: false,
            translation_required: false,
            native_names: false,
            error: None,
        }
    }

    pub fn what_not_filled(&self) -> i32 {
        let mut result = 0;
        let scans = self.files.get(&FileType::Scan).map(|v| v.len()).unwrap_or(0);
        let front = self.files.get(&FileType::FrontSide).map(|v| v.len()).unwrap_or(0);
        if self.fields.is_empty() && scans == 0 && front == 0 {
            result |= NOTHING_FILLED;
        }
        if self.translation_required {
            let translations = self.files.get(&FileType::Translation).map(|v| v.len()).unwrap_or(0);
            if translations == 0 {
                result |= NO_TRANSLATION_FILLED;
            }
        }
        if self.selfie_required {
            let selfies = self.files.get(&FileType::Selfie).map(|v| v.len()).unwrap_or(0);
            if selfies == 0 {
                result |= NO_SELFIE_FILLED;
            }
        }
        result
    }

    pub fn is_filled(&self) -> bool {
        self.what_not_filled() == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormRequest {
    pub bot_id: u64,
    pub scope: String,
    pub callback_url: String,
    pub public_key: String,
    pub nonce: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Form {
    pub values: BTreeMap<ValueType, Value>,
    pub requested_groups: Vec<Vec<ValueType>>,
    pub privacy_policy_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassportFormController {
    pub request: FormRequest,
    pub form: Form,
    pub secret: Option<Vec<u8>>,
    pub secret_id: u64,
    pub is_submitting: bool,
}

impl PassportFormController {
    pub fn new(request: FormRequest) -> Self {
        Self {
            request,
            form: Form::default(),
            secret: None,
            secret_id: 0,
            is_submitting: false,
        }
    }

    pub fn set_secret(&mut self, secret: Vec<u8>, secret_id: u64) {
        self.secret = Some(secret);
        self.secret_id = secret_id;
    }

    pub fn add_value(&mut self, value: Value) {
        if let Some(vt) = value.value_type {
            self.form.values.insert(vt, value);
        }
    }

    pub fn update_field(&mut self, value_type: ValueType, key: String, val: String) {
        let entry = self.form.values.entry(value_type).or_insert_with(|| Value::new(value_type));
        entry.fields.insert(key, val);
    }

    pub fn can_submit(&self) -> bool {
        if self.form.requested_groups.is_empty() {
            return !self.form.values.is_empty();
        }
        // At least one alternative in each requested group must be filled
        self.form.requested_groups.iter().all(|group| {
            group.iter().any(|vt| {
                self.form.values.get(vt).map(|v| v.is_filled()).unwrap_or(false)
            })
        })
    }

    pub fn missing_requirements(&self) -> Vec<String> {
        let mut missing = Vec::new();
        for (i, group) in self.form.requested_groups.iter().enumerate() {
            let filled = group.iter().any(|vt| {
                self.form.values.get(vt).map(|v| v.is_filled()).unwrap_or(false)
            });
            if !filled {
                let options: Vec<String> = group.iter().map(|vt| format!("{vt:?}")).collect();
                missing.push(format!("Requirement group #{}: need one of [{}]", i + 1, options.join(", ")));
            }
        }
        missing
    }
}
