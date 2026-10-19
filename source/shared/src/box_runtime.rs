#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SandBoxRuntime {
    #[default]
    Remote,
    LocalDocker,
}

impl SandBoxRuntime {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "remote" => Some(Self::Remote),
            "local-docker" => Some(Self::LocalDocker),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Remote => "remote",
            Self::LocalDocker => "local-docker",
        }
    }
}

pub const DEFAULT_SAND_BOX_RUNTIME: SandBoxRuntime = SandBoxRuntime::Remote;

pub fn is_sand_box_runtime(value: &str) -> bool {
    SandBoxRuntime::parse(value).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_supported_box_runtimes_parse() {
        assert_eq!(SandBoxRuntime::parse("remote"), Some(SandBoxRuntime::Remote));
        assert_eq!(SandBoxRuntime::parse("local-docker"), Some(SandBoxRuntime::LocalDocker));
        assert_eq!(SandBoxRuntime::parse("docker"), None);
    }
}
