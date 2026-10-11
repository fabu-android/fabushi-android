#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowserBridgeCommand { OpenHttps { url: String } }

impl BrowserBridgeCommand {
    pub fn open(url: impl Into<String>) -> Result<Self, &'static str> {
        let url = url.into();
        if !url.starts_with("https://") || url.len() > 8192 { return Err("only bounded HTTPS URLs are allowed"); }
        Ok(Self::OpenHttps { url })
    }
}
