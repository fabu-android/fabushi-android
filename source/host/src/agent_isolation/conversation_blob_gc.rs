use std::collections::HashSet;

include!(concat!(env!("OUT_DIR"), "/conversation_blob_gc_metadata.rs"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReachableBlobWalk {
    pub reachable_hex_ids: HashSet<String>,
    pub unresolved_proto_refs: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProtoDecodeError {
    MissingMessageDescriptor,
    Truncated,
    VarintOverflow,
    LengthOverflow,
    UnsupportedWireType,
}

struct BlobGraphWalker<'a, Lookup>
where
    Lookup: FnMut(&str) -> Option<Vec<u8>>,
{
    lookup: &'a mut Lookup,
    reachable_hex_ids: HashSet<String>,
    unresolved_proto_refs: usize,
}

impl<'a, Lookup> BlobGraphWalker<'a, Lookup>
where
    Lookup: FnMut(&str) -> Option<Vec<u8>>,
{
    fn visit_blob_reference(&mut self, blob_id: &[u8], blob_reference_type: &str) {
        if blob_id.is_empty() {
            return;
        }
        let hex_id = to_hex(blob_id);
        if !self.reachable_hex_ids.insert(hex_id.clone()) {
            return;
        }
        if !is_proto_blob_reference_type(blob_reference_type) {
            return;
        }
        let Some(child_bytes) = (self.lookup)(&hex_id) else {
            self.unresolved_proto_refs = self.unresolved_proto_refs.saturating_add(1);
            return;
        };
        if self.visit_message(&child_bytes, blob_reference_type).is_err() {
            self.unresolved_proto_refs = self.unresolved_proto_refs.saturating_add(1);
        }
    }

    fn visit_message(
        &mut self,
        bytes: &[u8],
        message_type: &str,
    ) -> Result<(), ProtoDecodeError> {
        let descriptor =
            message_descriptor(message_type).ok_or(ProtoDecodeError::MissingMessageDescriptor)?;
        for (field_number, value) in length_delimited_fields(bytes)? {
            if let Some(blob_field) = descriptor
                .blob_fields
                .iter()
                .find(|field| field.field_number == field_number)
            {
                if blob_field.skip {
                    continue;
                }
                if blob_field.map_value {
                    for (map_field_number, map_value) in length_delimited_fields(value)? {
                        if map_field_number == 2 {
                            self.visit_blob_reference(map_value, blob_field.blob_reference_type);
                        }
                    }
                } else {
                    self.visit_blob_reference(value, blob_field.blob_reference_type);
                }
                continue;
            }

            for nested in descriptor
                .nested_fields
                .iter()
                .filter(|field| field.field_number == field_number)
            {
                if nested.map_value {
                    for (map_field_number, map_value) in length_delimited_fields(value)? {
                        if map_field_number == 2 {
                            self.visit_message(map_value, nested.child_type)?;
                        }
                    }
                } else {
                    self.visit_message(value, nested.child_type)?;
                }
            }
        }
        Ok(())
    }
}

pub fn collect_reachable_blob_hex_ids(
    root_bytes: &[u8],
    mut get_blob_by_hex_id: impl FnMut(&str) -> Option<Vec<u8>>,
) -> Result<ReachableBlobWalk, ()> {
    let mut walker = BlobGraphWalker {
        lookup: &mut get_blob_by_hex_id,
        reachable_hex_ids: HashSet::new(),
        unresolved_proto_refs: 0,
    };
    walker
        .visit_message(root_bytes, "agent.v1.ConversationStateStructure")
        .map_err(|_| ())?;
    Ok(ReachableBlobWalk {
        reachable_hex_ids: walker.reachable_hex_ids,
        unresolved_proto_refs: walker.unresolved_proto_refs,
    })
}

fn length_delimited_fields(bytes: &[u8]) -> Result<Vec<(u64, &[u8])>, ProtoDecodeError> {
    let mut position = 0usize;
    let mut fields = Vec::new();
    while position < bytes.len() {
        let tag = read_varint(bytes, &mut position)?;
        let field_number = tag >> 3;
        let wire_type = (tag & 0x07) as u8;
        match wire_type {
            0 => {
                let _ = read_varint(bytes, &mut position)?;
            }
            1 => {
                position = position
                    .checked_add(8)
                    .ok_or(ProtoDecodeError::LengthOverflow)?;
                if position > bytes.len() {
                    return Err(ProtoDecodeError::Truncated);
                }
            }
            2 => {
                let length: usize = read_varint(bytes, &mut position)?
                    .try_into()
                    .map_err(|_| ProtoDecodeError::LengthOverflow)?;
                let end = position
                    .checked_add(length)
                    .ok_or(ProtoDecodeError::LengthOverflow)?;
                let value = bytes.get(position..end).ok_or(ProtoDecodeError::Truncated)?;
                fields.push((field_number, value));
                position = end;
            }
            5 => {
                position = position
                    .checked_add(4)
                    .ok_or(ProtoDecodeError::LengthOverflow)?;
                if position > bytes.len() {
                    return Err(ProtoDecodeError::Truncated);
                }
            }
            _ => return Err(ProtoDecodeError::UnsupportedWireType),
        }
    }
    Ok(fields)
}

fn read_varint(bytes: &[u8], position: &mut usize) -> Result<u64, ProtoDecodeError> {
    let mut value = 0u64;
    for shift in (0..70).step_by(7) {
        let byte = *bytes.get(*position).ok_or(ProtoDecodeError::Truncated)?;
        *position += 1;
        if shift == 63 && byte > 1 {
            return Err(ProtoDecodeError::VarintOverflow);
        }
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(ProtoDecodeError::VarintOverflow)
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
