//! Telegram Desktop Geolocation Coordinates & Reverse Geocoding
//! Upstream sources: `Telegram/SourceFiles/core/current_geo_location.h`, `current_geo_location.cpp`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GeoLocationAccuracy {
    Exact,
    Country,
    #[default]
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeoPoint {
    pub latitude: f64,
    pub longitude: f64,
}

/// Geolocation data container representing current user location or country bounds.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CurrentGeoLocation {
    pub point: Option<GeoPoint>,
    pub accuracy: GeoLocationAccuracy,
    pub country_code: Option<String>,
    pub address: Option<String>,
}

impl CurrentGeoLocation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn exact(&self) -> bool {
        self.accuracy == GeoLocationAccuracy::Exact
    }

    pub fn country(&self) -> bool {
        self.accuracy == GeoLocationAccuracy::Country
    }

    pub fn failed(&self) -> bool {
        self.accuracy == GeoLocationAccuracy::Failed
    }

    pub fn set_exact(&mut self, lat: f64, lon: f64) {
        self.point = Some(GeoPoint {
            latitude: lat,
            longitude: lon,
        });
        self.accuracy = GeoLocationAccuracy::Exact;
    }

    pub fn set_country(&mut self, code: impl Into<String>) {
        self.country_code = Some(code.into());
        self.accuracy = GeoLocationAccuracy::Country;
    }

    pub fn set_address(&mut self, addr: impl Into<String>) {
        self.address = Some(addr.into());
    }
}
