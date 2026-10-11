#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandUpdateTrack {
    Stable,
    Nightly,
    Dogfood,
}

impl SandUpdateTrack {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "stable" => Some(Self::Stable),
            "nightly" => Some(Self::Nightly),
            "dogfood" => Some(Self::Dogfood),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Nightly => "nightly",
            Self::Dogfood => "dogfood",
        }
    }
}

pub const SAND_UPDATE_TRACKS: &[&str] = &["stable", "nightly", "dogfood"];

pub fn is_sand_update_track(value: &str) -> bool {
    SandUpdateTrack::parse(value).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_known_update_tracks_are_accepted() {
        assert!(is_sand_update_track("stable"));
        assert!(is_sand_update_track("nightly"));
        assert!(is_sand_update_track("dogfood"));
        assert!(!is_sand_update_track("beta"));
    }
}
