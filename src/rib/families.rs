use std::collections::HashSet;
use std::convert::{From, TryFrom};
use std::fmt;

use bgp_rs::{OpenCapability, AFI, SAFI};
use serde::{self, Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Family {
    pub afi: AFI,
    pub safi: SAFI,
}

impl fmt::Display for Family {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {}", self.afi, self.safi)
    }
}

impl Family {
    pub fn new(afi: AFI, safi: SAFI) -> Self {
        Self { afi, safi }
    }

    pub fn to_open_param(self) -> OpenCapability {
        OpenCapability::MultiProtocol((self.afi, self.safi))
    }
}

impl From<&Family> for (AFI, SAFI) {
    fn from(family: &Family) -> (AFI, SAFI) {
        (family.afi, family.safi)
    }
}

impl TryFrom<(u16, u8)> for Family {
    type Error = std::io::Error;

    fn try_from(v: (u16, u8)) -> Result<Self, Self::Error> {
        Ok(Self::new(AFI::try_from(v.0)?, SAFI::try_from(v.1)?))
    }
}

impl Serialize for Family {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Family {
    fn deserialize<D>(deserializer: D) -> Result<Family, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let parts: Vec<&str> = s.trim().split_whitespace().collect();
        if parts.len() != 2 {
            return Err(serde::de::Error::custom(format!(
                "Incorrect family format: '{}'",
                s
            )));
        }
        let afi = match parts[0] {
            "ipv4" => AFI::IPV4,
            "ipv6" => AFI::IPV6,
            "l2vpn" => AFI::L2VPN,
            family => {
                return Err(serde::de::Error::custom(format!(
                    "Unsupported AFI: '{}'",
                    family
                )))
            }
        };
        let safi = match parts[1] {
            "unicast" => SAFI::Unicast,
            "flow" => SAFI::Flowspec,
            sfamily => {
                return Err(serde::de::Error::custom(format!(
                    "Unsupported SAFI: '{}'",
                    sfamily
                )))
            }
        };
        Ok(Family::new(afi, safi))
    }
}

#[derive(Debug, Clone)]
pub struct Families(HashSet<Family>);

impl Families {
    pub fn new(families: Vec<Family>) -> Self {
        Self(families.into_iter().collect())
    }

    pub fn common(&self, other: &Families) -> Self {
        Self(self.0.intersection(&other.0).cloned().collect())
    }

    pub fn contains(&self, family: Family) -> bool {
        self.0.contains(&family)
    }

    pub fn iter(&self) -> std::collections::hash_set::Iter<Family> {
        self.0.iter()
    }
}

impl From<Families> for HashSet<(AFI, SAFI)> {
    fn from(families: Families) -> HashSet<(AFI, SAFI)> {
        families
            .0
            .iter()
            .cloned()
            .map(|f| (f.afi, f.safi))
            .collect()
    }
}

impl From<&HashSet<(AFI, SAFI)>> for Families {
    fn from(mp_fams: &HashSet<(AFI, SAFI)>) -> Self {
        Self::new(
            mp_fams
                .iter()
                .cloned()
                .map(|f| Family::new(f.0, f.1))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::value::{Error, StrDeserializer};
    use serde::de::IntoDeserializer;

    #[test]
    fn test_serialize_family() {
        assert_eq!(
            &Family::new(AFI::IPV4, SAFI::Unicast).to_string(),
            "IPv4 Unicast"
        );
        assert_eq!(
            &Family::new(AFI::IPV6, SAFI::Flowspec).to_string(),
            "IPv6 Flowspec"
        );
    }
    #[test]
    fn test_deserialize_family() {
        let deserializer: StrDeserializer<Error> = "ipv6 unicast".into_deserializer();
        let familyi = Family::deserialize(deserializer).unwrap();
        assert_eq!(familyi, Family::new(AFI::IPV6, SAFI::Unicast));

        let deserializer: StrDeserializer<Error> = "ipv4 flow".into_deserializer();
        let familyi = Family::deserialize(deserializer).unwrap();
        assert_eq!(familyi, Family::new(AFI::IPV4, SAFI::Flowspec));
    }
}
