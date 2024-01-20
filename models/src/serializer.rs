use serde::Serializer;
use alloc::string::String;
use alloc::format;

pub fn serialize_u64_as_str<S>(v: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let sv = format!("{:x}", v);
    serializer.serialize_str(&sv)
}
