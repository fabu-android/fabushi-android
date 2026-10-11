#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarrierKind { AndroidBinder, InProcess }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoordinatorBootstrap {
    pub app_version: String,
    pub data_dir: String,
    pub packaged: bool,
}

impl CoordinatorBootstrap {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.app_version.trim().is_empty() { return Err("app_version must not be empty"); }
        if self.data_dir.trim().is_empty() { return Err("data_dir must not be empty"); }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarrierState {
    pub kind: CarrierKind,
    pub control_open: bool,
    pub data_open: bool,
    pub main_data_open: bool,
}

impl CarrierState {
    pub fn new(kind: CarrierKind) -> Self {
        Self { kind, control_open: true, data_open: true, main_data_open: true }
    }
    pub fn close_all(&mut self) {
        self.control_open = false;
        self.data_open = false;
        self.main_data_open = false;
    }
}
