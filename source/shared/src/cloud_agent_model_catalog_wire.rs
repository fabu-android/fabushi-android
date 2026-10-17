//! Desktop-compatible Cloud Agent USER_AVAILABLE model-catalog wire codec.
//!
//! The authenticated HTTP request remains a Host/authorized-backend responsibility. This module
//! owns only the stable protobuf request/response contract so Android does not invent a second
//! catalog schema.

use crate::model_catalog::{
    SandModelCatalogEntry, SandModelCatalogParameter, SandModelCatalogParameterType,
    SandModelCatalogParameterValue, SandModelCatalogValue,
};

pub const AVAILABLE_MODELS_SCOPE_USER_AVAILABLE: u64 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloudAgentModelCatalogWireError {
    InvalidProto(String),
}

impl std::fmt::Display for CloudAgentModelCatalogWireError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidProto(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for CloudAgentModelCatalogWireError {}

fn invalid(message: impl Into<String>) -> CloudAgentModelCatalogWireError {
    CloudAgentModelCatalogWireError::InvalidProto(message.into())
}

fn push_varint(output: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            return;
        }
    }
}

fn push_varint_field(output: &mut Vec<u8>, field: u64, value: u64) {
    push_varint(output, field << 3);
    push_varint(output, value);
}

/// Desktop sends a USER_AVAILABLE catalog request with fields 5=true, 7=true and scope=1.
pub fn encode_cloud_agent_available_models_request() -> Vec<u8> {
    let mut output = Vec::with_capacity(6);
    push_varint_field(&mut output, 5, 1);
    push_varint_field(&mut output, 7, 1);
    push_varint_field(&mut output, 10, AVAILABLE_MODELS_SCOPE_USER_AVAILABLE);
    output
}

fn read_varint(
    input: &[u8],
    cursor: &mut usize,
) -> Result<u64, CloudAgentModelCatalogWireError> {
    let mut value = 0_u64;
    let mut shift = 0_u32;
    while *cursor < input.len() && shift < 64 {
        let byte = input[*cursor];
        *cursor += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
    }
    Err(invalid("malformed varint"))
}

fn read_bytes<'a>(
    input: &'a [u8],
    cursor: &mut usize,
) -> Result<&'a [u8], CloudAgentModelCatalogWireError> {
    let length = usize::try_from(read_varint(input, cursor)?)
        .map_err(|_| invalid("length overflow"))?;
    let end = cursor
        .checked_add(length)
        .ok_or_else(|| invalid("length overflow"))?;
    if end > input.len() {
        return Err(invalid("truncated length-delimited field"));
    }
    let value = &input[*cursor..end];
    *cursor = end;
    Ok(value)
}

fn read_string(
    input: &[u8],
    cursor: &mut usize,
) -> Result<String, CloudAgentModelCatalogWireError> {
    let bytes = read_bytes(input, cursor)?;
    String::from_utf8(bytes.to_vec()).map_err(|_| invalid("string field is not UTF-8"))
}

fn skip_field(
    input: &[u8],
    cursor: &mut usize,
    wire: u8,
) -> Result<(), CloudAgentModelCatalogWireError> {
    match wire {
        0 => {
            let _ = read_varint(input, cursor)?;
        }
        1 => {
            *cursor = cursor.checked_add(8).ok_or_else(|| invalid("length overflow"))?;
        }
        2 => {
            let _ = read_bytes(input, cursor)?;
        }
        5 => {
            *cursor = cursor.checked_add(4).ok_or_else(|| invalid("length overflow"))?;
        }
        other => return Err(invalid(format!("unsupported wire type {other}"))),
    }
    if *cursor > input.len() {
        return Err(invalid("truncated protobuf field"));
    }
    Ok(())
}

fn clean_optional(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn parse_parameter_value(
    input: &[u8],
) -> Result<SandModelCatalogValue, CloudAgentModelCatalogWireError> {
    let mut cursor = 0;
    let mut value = String::new();
    let mut display_name = None;
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = key >> 3;
        let wire = (key & 7) as u8;
        match (field, wire) {
            (1, 2) => value = read_string(input, &mut cursor)?,
            (2, 2) => display_name = clean_optional(read_string(input, &mut cursor)?),
            _ => skip_field(input, &mut cursor, wire)?,
        }
    }
    Ok(SandModelCatalogValue {
        value,
        display_name,
    })
}

fn parse_parameter_values(
    input: &[u8],
) -> Result<Vec<SandModelCatalogValue>, CloudAgentModelCatalogWireError> {
    let mut cursor = 0;
    let mut values = Vec::new();
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = key >> 3;
        let wire = (key & 7) as u8;
        if field == 1 && wire == 2 {
            values.push(parse_parameter_value(read_bytes(input, &mut cursor)?)?);
        } else {
            skip_field(input, &mut cursor, wire)?;
        }
    }
    Ok(values)
}

fn parse_parameter_type(
    input: &[u8],
) -> Result<
    Option<(SandModelCatalogParameterType, Vec<SandModelCatalogValue>)>,
    CloudAgentModelCatalogWireError,
> {
    let mut cursor = 0;
    let mut result = None;
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = key >> 3;
        let wire = (key & 7) as u8;
        match (field, wire) {
            (1, 2) => {
                result = Some((
                    SandModelCatalogParameterType::Boolean,
                    parse_parameter_values(read_bytes(input, &mut cursor)?)?,
                ));
            }
            (2, 2) => {
                result = Some((
                    SandModelCatalogParameterType::Enum,
                    parse_parameter_values(read_bytes(input, &mut cursor)?)?,
                ));
            }
            _ => skip_field(input, &mut cursor, wire)?,
        }
    }
    Ok(result)
}

fn parse_parameter_definition(
    input: &[u8],
) -> Result<Option<SandModelCatalogParameter>, CloudAgentModelCatalogWireError> {
    let mut cursor = 0;
    let mut id = String::new();
    let mut name = None;
    let mut parameter = None;
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = key >> 3;
        let wire = (key & 7) as u8;
        match (field, wire) {
            (1, 2) => id = read_string(input, &mut cursor)?,
            (2, 2) => name = clean_optional(read_string(input, &mut cursor)?),
            (4, 2) => parameter = parse_parameter_type(read_bytes(input, &mut cursor)?)?,
            _ => skip_field(input, &mut cursor, wire)?,
        }
    }
    let Some((parameter_type, values)) = parameter else {
        return Ok(None);
    };
    if id.trim().is_empty() || values.is_empty() {
        return Ok(None);
    }
    Ok(Some(SandModelCatalogParameter {
        id,
        name,
        parameter_type,
        values,
    }))
}

fn parse_variant_parameter_value(
    input: &[u8],
) -> Result<SandModelCatalogParameterValue, CloudAgentModelCatalogWireError> {
    let mut cursor = 0;
    let mut id = String::new();
    let mut value = String::new();
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = key >> 3;
        let wire = (key & 7) as u8;
        match (field, wire) {
            (1, 2) => id = read_string(input, &mut cursor)?,
            (2, 2) => value = read_string(input, &mut cursor)?,
            _ => skip_field(input, &mut cursor, wire)?,
        }
    }
    Ok(SandModelCatalogParameterValue { id, value })
}

fn parse_variant(
    input: &[u8],
) -> Result<Vec<SandModelCatalogParameterValue>, CloudAgentModelCatalogWireError> {
    let mut cursor = 0;
    let mut values = Vec::new();
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = key >> 3;
        let wire = (key & 7) as u8;
        if field == 1 && wire == 2 {
            values.push(parse_variant_parameter_value(read_bytes(input, &mut cursor)?)?);
        } else {
            skip_field(input, &mut cursor, wire)?;
        }
    }
    Ok(values)
}

fn parse_available_model(
    input: &[u8],
) -> Result<Option<SandModelCatalogEntry>, CloudAgentModelCatalogWireError> {
    let mut cursor = 0;
    let mut id = String::new();
    let mut display_name = None;
    let mut aliases = Vec::new();
    let mut params = Vec::new();
    let mut variants = Vec::new();
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = key >> 3;
        let wire = (key & 7) as u8;
        match (field, wire) {
            (1, 2) => id = read_string(input, &mut cursor)?,
            (17, 2) => display_name = clean_optional(read_string(input, &mut cursor)?),
            (29, 2) => {
                if let Some(parameter) =
                    parse_parameter_definition(read_bytes(input, &mut cursor)?)?
                {
                    params.push(parameter);
                }
            }
            (30, 2) => variants.push(parse_variant(read_bytes(input, &mut cursor)?)?),
            (37, 2) => aliases.push(read_string(input, &mut cursor)?),
            _ => skip_field(input, &mut cursor, wire)?,
        }
    }
    if id.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(SandModelCatalogEntry {
        id,
        display_name,
        aliases,
        params,
        variants,
    }))
}

pub fn decode_cloud_agent_available_models_response(
    input: &[u8],
) -> Result<Vec<SandModelCatalogEntry>, CloudAgentModelCatalogWireError> {
    let mut cursor = 0;
    let mut models = Vec::new();
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = key >> 3;
        let wire = (key & 7) as u8;
        if field == 2 && wire == 2 {
            if let Some(model) = parse_available_model(read_bytes(input, &mut cursor)?)? {
                models.push(model);
            }
        } else {
            skip_field(input, &mut cursor, wire)?;
        }
    }
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_bytes_field(out: &mut Vec<u8>, field: u64, bytes: &[u8]) {
        push_varint(out, (field << 3) | 2);
        push_varint(out, bytes.len() as u64);
        out.extend_from_slice(bytes);
    }

    fn string_field(field: u64, value: &str) -> Vec<u8> {
        let mut out = Vec::new();
        push_bytes_field(&mut out, field, value.as_bytes());
        out
    }

    #[test]
    fn request_exactly_uses_desktop_user_available_scope() {
        assert_eq!(
            encode_cloud_agent_available_models_request(),
            vec![0x28, 0x01, 0x38, 0x01, 0x50, 0x01]
        );
    }

    #[test]
    fn decodes_model_identity_alias_parameter_and_variant() {
        let mut parameter_value = string_field(1, "true");
        parameter_value.extend(string_field(2, "Enabled"));
        let mut values = Vec::new();
        push_bytes_field(&mut values, 1, &parameter_value);
        let mut bool_type = Vec::new();
        push_bytes_field(&mut bool_type, 1, &values);

        let mut parameter = string_field(1, "fast");
        parameter.extend(string_field(2, "Fast"));
        push_bytes_field(&mut parameter, 4, &bool_type);

        let mut variant_value = string_field(1, "fast");
        variant_value.extend(string_field(2, "true"));
        let mut variant = Vec::new();
        push_bytes_field(&mut variant, 1, &variant_value);

        let mut model = string_field(1, "grok-4.5");
        model.extend(string_field(17, "Grok 4.5"));
        push_bytes_field(&mut model, 29, &parameter);
        push_bytes_field(&mut model, 30, &variant);
        model.extend(string_field(37, "grok"));

        let mut response = Vec::new();
        push_bytes_field(&mut response, 2, &model);

        let decoded = decode_cloud_agent_available_models_response(&response).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].id, "grok-4.5");
        assert_eq!(decoded[0].display_name.as_deref(), Some("Grok 4.5"));
        assert_eq!(decoded[0].aliases, vec!["grok"]);
        assert_eq!(decoded[0].params[0].id, "fast");
        assert_eq!(decoded[0].params[0].values[0].value, "true");
        assert_eq!(decoded[0].variants[0][0].value, "true");
    }

    #[test]
    fn malformed_wire_fails_closed() {
        assert!(decode_cloud_agent_available_models_response(&[0x12, 0xff]).is_err());
        assert!(decode_cloud_agent_available_models_response(&[0x0f]).is_err());
    }
}
