//! Conversion from zvariant values into the pure [`HintValue`] type.

use std::collections::HashMap;

use zbus::zvariant::Value;

use crate::model::image::IMAGE_MAX_BYTES;
use crate::model::limits::HINTS_MAX;
use crate::model::{HintValue, RawImage};

/// Converts the `a{sv}` hints dictionary, ignoring entries past the cap.
pub fn hint_pairs(hints: &HashMap<&str, Value<'_>>) -> Vec<(String, HintValue)> {
    hints
        .iter()
        .take(HINTS_MAX)
        .map(|(key, value)| ((*key).to_string(), hint_value(value)))
        .collect()
}

fn hint_value(value: &Value<'_>) -> HintValue {
    match value {
        Value::Bool(flag) => HintValue::Bool(*flag),
        Value::U8(raw) => HintValue::Byte(*raw),
        Value::I32(raw) => HintValue::Int(*raw),
        Value::U32(raw) => HintValue::UInt(*raw),
        Value::Str(text) => HintValue::Text(text.as_str().to_string()),
        Value::Structure(structure) => {
            raw_image(structure.fields()).map_or(HintValue::Other, HintValue::Image)
        }
        _ => HintValue::Other,
    }
}

/// Reads the `(iiibiiay)` image structure. Oversize payloads are rejected before copying.
fn raw_image(fields: &[Value<'_>]) -> Option<RawImage> {
    let [
        Value::I32(width),
        Value::I32(height),
        Value::I32(rowstride),
        Value::Bool(has_alpha),
        Value::I32(bits_per_sample),
        Value::I32(channels),
        Value::Array(data),
    ] = fields
    else {
        return None;
    };
    if data.len() > IMAGE_MAX_BYTES {
        return None;
    }
    let bytes = data
        .iter()
        .map(|byte| match byte {
            Value::U8(raw) => Some(*raw),
            _ => None,
        })
        .collect::<Option<Vec<u8>>>()?;
    Some(RawImage {
        width: *width,
        height: *height,
        rowstride: *rowstride,
        has_alpha: *has_alpha,
        bits_per_sample: *bits_per_sample,
        channels: *channels,
        data: bytes,
    })
}
