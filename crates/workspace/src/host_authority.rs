//! Validated hostname[:port] metadata shared by connector configuration and persistence.
use serde::{Deserialize, Deserializer, Serialize};
use std::{fmt, str::FromStr};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct HostAuthority(String);
impl HostAuthority {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl FromStr for HostAuthority {
    type Err = &'static str;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let invalid = "地址须为 hostname[:port]，端口范围 1–65535。";
        let (host, port) = value
            .split_once(':')
            .map_or((value, None), |(h, p)| (h, Some(p)));
        if host.is_empty()
            || host.len() > 253
            || host.split('.').any(|label| {
                label.is_empty()
                    || label.len() > 63
                    || !label.as_bytes()[0].is_ascii_alphanumeric()
                    || !label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                    || !label
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            })
            || port.is_some_and(|p| {
                p.is_empty()
                    || !p.bytes().all(|c| c.is_ascii_digit())
                    || !p.parse::<u16>().is_ok_and(|p| p > 0)
            })
        {
            return Err(invalid);
        }
        Ok(Self(value.to_ascii_lowercase()))
    }
}
impl fmt::Display for HostAuthority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl<'de> Deserialize<'de> for HostAuthority {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}
