use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Request {
    About,
    Report,
}
impl Request {
    pub fn parse(args: &[String]) -> anyhow::Result<Self> {
        match args
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .as_slice()
        {
            [] | ["--about"] => Ok(Self::About),
            ["--report"] => Ok(Self::Report),
            _ => anyhow::bail!("Usage: macstill-info [--about|--report|--json]"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn entry_points_are_explicit_and_invalid_arguments_are_rejected() {
        assert_eq!(Request::parse(&[]).unwrap(), Request::About);
        assert_eq!(
            Request::parse(&["--report".into()]).unwrap(),
            Request::Report
        );
        assert!(Request::parse(&["--report".into(), "unexpected".into()]).is_err());
    }
}
