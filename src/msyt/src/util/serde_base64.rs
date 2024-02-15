use serde::de::{self, Visitor};

use std::fmt::{self, Formatter};

pub struct Base64Visitor;

impl<'de> Visitor<'de> for Base64Visitor {
  type Value = Vec<u8>;

  fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
    formatter.write_str("a string")
  }

  fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where E: de::Error,
  {
    base64::decode(v)
      .map_err(|_| de::Error::invalid_value(de::Unexpected::Str(v), &"valid base64"))
  }
}

// pub fn serialize<T, S>(data: &T, ser: S) -> Result<S::Ok, S::Error>
//   where S: Serializer,
//         T: AsRef<[u8]> + ?Sized,
// {
//   ser.serialize_str(&base64::encode(data))
// }

// pub fn deserialize<'de, D>(des: D) -> Result<Vec<u8>, D::Error>
//   where D: Deserializer<'de>,
// {
//   des.deserialize_string(Base64Visitor)
// }
