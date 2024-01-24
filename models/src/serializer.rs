use alloc::format;
use serde::Serializer;

pub fn serialize_u64_as_str<S>(v: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let sv = format!("0x{:x}", v);
    serializer.serialize_str(&sv)
}
