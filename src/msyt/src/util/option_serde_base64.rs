use serde::de::{self, Deserializer, Visitor};
use serde::ser::Serializer;

use std::fmt::{self, Formatter};

pub struct OptionBase64Visitor;

impl<'de> Visitor<'de> for OptionBase64Visitor {
  type Value = Option<Vec<u8>>;

  fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
    formatter.write_str("a string or null")
  }

  fn visit_none<E>(self) -> Result<Self::Value, E>
    where E: de::Error,
  {
    Ok(None)
  }

  fn visit_some<D>(self, des: D) -> Result<Self::Value, D::Error>
    where D: Deserializer<'de>,
  {
    des.deserialize_string(super::serde_base64::Base64Visitor).map(Some)
  }
}

pub fn serialize<T, S>(data: &Option<T>, ser: S) -> Result<S::Ok, S::Error>
  where S: Serializer,
        T: AsRef<[u8]>,
{
  match *data {
    Some(ref data) => ser.serialize_some(&base64::encode(data)),
    None => ser.serialize_none(),
  }
}

pub fn deserialize<'de, D>(des: D) -> Result<Option<Vec<u8>>, D::Error>
  where D: Deserializer<'de>,
{
  des.deserialize_option(OptionBase64Visitor)
}
