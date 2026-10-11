#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GatewayError {
    Command(String),
    Unreachable { kind: String, message: String },
    Transport(String),
}
