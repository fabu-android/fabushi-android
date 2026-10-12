const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TranscriptMirrorConversationState {
    pub root_prompt_messages_json: Vec<Vec<u8>>,
    pub turns: Vec<Vec<u8>>,
    pub summary_archives: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConversationTokenDetailsFields {
    pub used_tokens: u64,
    pub max_tokens: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConversationStepTimingFields {
    pub duration_ms: u64,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConversationStateRecoveryFields {
    pub root_prompt_messages_json: Vec<Vec<u8>>,
    pub turns: Vec<Vec<u8>>,
    pub todos: Vec<Vec<u8>>,
    pub token_details: Option<ConversationTokenDetailsFields>,
    pub summary: Option<Vec<u8>>,
    pub summary_archives: Vec<Vec<u8>>,
    pub turn_timings: Vec<ConversationStepTimingFields>,
    pub self_summary_count: u64,
    pub subagent_states: Vec<(String, Vec<u8>)>,
    pub subagent_state_refs: Vec<(String, Vec<u8>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationTurnStructureFields {
    Agent {
        user_message: Vec<u8>,
        steps: Vec<Vec<u8>>,
        request_id: Option<String>,
    },
    Shell {
        shell_command: Vec<u8>,
        shell_output: Vec<u8>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SubagentPersistedStateFields {
    pub conversation_state: Option<Vec<u8>>,
    pub created_timestamp_ms: u64,
    pub last_used_timestamp_ms: u64,
    pub subagent_type: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TranscriptMirrorProtobufDecodeError {
    #[error("truncated protobuf varint")]
    TruncatedVarint,
    #[error("protobuf varint exceeds the safe integer range")]
    UnsafeVarint,
    #[error("invalid protobuf varint")]
    InvalidVarint,
    #[error("truncated protobuf bytes field")]
    TruncatedBytes,
    #[error("mismatched protobuf group")]
    MismatchedGroup,
    #[error("unterminated protobuf group")]
    UnterminatedGroup,
    #[error("unexpected protobuf end group")]
    UnexpectedEndGroup,
    #[error("invalid protobuf wire type")]
    InvalidWireType,
    #[error("truncated protobuf field")]
    TruncatedField,
    #[error("invalid protobuf field number")]
    InvalidFieldNumber,
    #[error("protobuf string field is not valid UTF-8")]
    InvalidUtf8,
}

#[derive(Debug, Clone, Copy)]
struct Cursor {
    offset: usize,
}

fn read_varint(
    bytes: &[u8],
    cursor: &mut Cursor,
) -> Result<u64, TranscriptMirrorProtobufDecodeError> {
    let mut value = 0u64;
    for shift in (0..70).step_by(7) {
        let byte = *bytes
            .get(cursor.offset)
            .ok_or(TranscriptMirrorProtobufDecodeError::TruncatedVarint)?;
        cursor.offset += 1;
        let payload = u64::from(byte & 0x7f);
        if shift == 63 && payload > 1 {
            return Err(TranscriptMirrorProtobufDecodeError::InvalidVarint);
        }
        value |= payload << shift;
        if byte & 0x80 == 0 {
            if value > MAX_SAFE_INTEGER {
                return Err(TranscriptMirrorProtobufDecodeError::UnsafeVarint);
            }
            return Ok(value);
        }
    }
    Err(TranscriptMirrorProtobufDecodeError::InvalidVarint)
}

fn read_bytes(
    bytes: &[u8],
    cursor: &mut Cursor,
) -> Result<Vec<u8>, TranscriptMirrorProtobufDecodeError> {
    let length = read_varint(bytes, cursor)?;
    let length: usize = length
        .try_into()
        .map_err(|_| TranscriptMirrorProtobufDecodeError::TruncatedBytes)?;
    let end = cursor
        .offset
        .checked_add(length)
        .ok_or(TranscriptMirrorProtobufDecodeError::TruncatedBytes)?;
    if end > bytes.len() {
        return Err(TranscriptMirrorProtobufDecodeError::TruncatedBytes);
    }
    let value = bytes[cursor.offset..end].to_vec();
    cursor.offset = end;
    Ok(value)
}

fn skip_field(
    bytes: &[u8],
    cursor: &mut Cursor,
    wire_type: u8,
    field_number: u64,
) -> Result<(), TranscriptMirrorProtobufDecodeError> {
    match wire_type {
        0 => {
            let _ = read_varint(bytes, cursor)?;
            return Ok(());
        }
        1 => {
            cursor.offset = cursor
                .offset
                .checked_add(8)
                .ok_or(TranscriptMirrorProtobufDecodeError::TruncatedField)?;
        }
        2 => {
            let length = read_varint(bytes, cursor)?;
            let length: usize = length
                .try_into()
                .map_err(|_| TranscriptMirrorProtobufDecodeError::TruncatedField)?;
            cursor.offset = cursor
                .offset
                .checked_add(length)
                .ok_or(TranscriptMirrorProtobufDecodeError::TruncatedField)?;
        }
        3 => {
            while cursor.offset < bytes.len() {
                let tag = read_varint(bytes, cursor)?;
                let nested_field = tag / 8;
                let nested_wire = (tag & 7) as u8;
                if nested_wire == 4 {
                    if nested_field != field_number {
                        return Err(TranscriptMirrorProtobufDecodeError::MismatchedGroup);
                    }
                    return Ok(());
                }
                skip_field(bytes, cursor, nested_wire, nested_field)?;
            }
            return Err(TranscriptMirrorProtobufDecodeError::UnterminatedGroup);
        }
        4 => return Err(TranscriptMirrorProtobufDecodeError::UnexpectedEndGroup),
        5 => {
            cursor.offset = cursor
                .offset
                .checked_add(4)
                .ok_or(TranscriptMirrorProtobufDecodeError::TruncatedField)?;
        }
        _ => return Err(TranscriptMirrorProtobufDecodeError::InvalidWireType),
    }
    if cursor.offset > bytes.len() {
        return Err(TranscriptMirrorProtobufDecodeError::TruncatedField);
    }
    Ok(())
}


fn read_string(
    bytes: &[u8],
    cursor: &mut Cursor,
) -> Result<String, TranscriptMirrorProtobufDecodeError> {
    let value = read_bytes(bytes, cursor)?;
    String::from_utf8(value).map_err(|_| TranscriptMirrorProtobufDecodeError::InvalidUtf8)
}

fn decode_string_bytes_map_entry(
    bytes: &[u8],
) -> Result<Option<(String, Vec<u8>)>, TranscriptMirrorProtobufDecodeError> {
    let mut cursor = Cursor { offset: 0 };
    let mut key = None;
    let mut value = None;
    while cursor.offset < bytes.len() {
        let tag = read_varint(bytes, &mut cursor)?;
        let field_number = tag / 8;
        let wire_type = (tag & 7) as u8;
        match (field_number, wire_type) {
            (1, 2) => key = Some(read_string(bytes, &mut cursor)?),
            (2, 2) => value = Some(read_bytes(bytes, &mut cursor)?),
            _ => skip_field(bytes, &mut cursor, wire_type, field_number)?,
        }
    }
    Ok(key.zip(value))
}

fn decode_conversation_token_details(
    bytes: &[u8],
) -> Result<ConversationTokenDetailsFields, TranscriptMirrorProtobufDecodeError> {
    let mut cursor = Cursor { offset: 0 };
    let mut details = ConversationTokenDetailsFields::default();
    while cursor.offset < bytes.len() {
        let tag = read_varint(bytes, &mut cursor)?;
        let field_number = tag / 8;
        let wire_type = (tag & 7) as u8;
        match (field_number, wire_type) {
            (1, 0) => details.used_tokens = read_varint(bytes, &mut cursor)?,
            (2, 0) => details.max_tokens = read_varint(bytes, &mut cursor)?,
            _ => skip_field(bytes, &mut cursor, wire_type, field_number)?,
        }
    }
    Ok(details)
}

fn decode_conversation_step_timing(
    bytes: &[u8],
) -> Result<ConversationStepTimingFields, TranscriptMirrorProtobufDecodeError> {
    let mut cursor = Cursor { offset: 0 };
    let mut timing = ConversationStepTimingFields::default();
    while cursor.offset < bytes.len() {
        let tag = read_varint(bytes, &mut cursor)?;
        let field_number = tag / 8;
        let wire_type = (tag & 7) as u8;
        match (field_number, wire_type) {
            (1, 0) => timing.duration_ms = read_varint(bytes, &mut cursor)?,
            (2, 0) => timing.timestamp_ms = read_varint(bytes, &mut cursor)?,
            _ => skip_field(bytes, &mut cursor, wire_type, field_number)?,
        }
    }
    Ok(timing)
}

pub fn decode_conversation_state_recovery_fields(
    bytes: &[u8],
) -> Result<ConversationStateRecoveryFields, TranscriptMirrorProtobufDecodeError> {
    let mut cursor = Cursor { offset: 0 };
    let mut state = ConversationStateRecoveryFields::default();

    while cursor.offset < bytes.len() {
        let tag = read_varint(bytes, &mut cursor)?;
        let field_number = tag / 8;
        let wire_type = (tag & 7) as u8;
        if field_number == 0 {
            return Err(TranscriptMirrorProtobufDecodeError::InvalidFieldNumber);
        }
        if field_number == 17 && wire_type == 0 {
            state.self_summary_count = read_varint(bytes, &mut cursor)?;
            continue;
        }
        if wire_type == 2 {
            match field_number {
                1 => {
                    state.root_prompt_messages_json.push(read_bytes(bytes, &mut cursor)?);
                    continue;
                }
                3 => {
                    state.todos.push(read_bytes(bytes, &mut cursor)?);
                    continue;
                }
                5 => {
                    let token_details = read_bytes(bytes, &mut cursor)?;
                    state.token_details = Some(decode_conversation_token_details(&token_details)?);
                    continue;
                }
                6 => {
                    state.summary = Some(read_bytes(bytes, &mut cursor)?);
                    continue;
                }
                8 => {
                    state.turns.push(read_bytes(bytes, &mut cursor)?);
                    continue;
                }
                13 => {
                    state.summary_archives.push(read_bytes(bytes, &mut cursor)?);
                    continue;
                }
                14 => {
                    let timing = read_bytes(bytes, &mut cursor)?;
                    state.turn_timings.push(decode_conversation_step_timing(&timing)?);
                    continue;
                }
                16 => {
                    let entry = read_bytes(bytes, &mut cursor)?;
                    if let Some(entry) = decode_string_bytes_map_entry(&entry)? {
                        state.subagent_states.push(entry);
                    }
                    continue;
                }
                31 => {
                    let entry = read_bytes(bytes, &mut cursor)?;
                    if let Some(entry) = decode_string_bytes_map_entry(&entry)? {
                        state.subagent_state_refs.push(entry);
                    }
                    continue;
                }
                _ => {}
            }
        }
        skip_field(bytes, &mut cursor, wire_type, field_number)?;
    }

    Ok(state)
}

/// Monotonic compaction epoch derived from the durable Agent conversation state.
///
/// Grok treats both external summary archives and self summaries as completed
/// compactions. Their persisted counts therefore form the turn-visible epoch
/// used to decide whether unchanged dynamic prompt state must be re-injected.
pub fn conversation_compaction_epoch(
    bytes: &[u8],
) -> Result<u64, TranscriptMirrorProtobufDecodeError> {
    let decoded = decode_conversation_state_recovery_fields(bytes)?;
    Ok((decoded.summary_archives.len() as u64).saturating_add(decoded.self_summary_count))
}

pub fn remove_conversation_state_token_details(
    bytes: &[u8],
) -> Result<Vec<u8>, TranscriptMirrorProtobufDecodeError> {
    let mut cursor = Cursor { offset: 0 };
    let mut output = Vec::with_capacity(bytes.len());
    while cursor.offset < bytes.len() {
        let field_start = cursor.offset;
        let tag = read_varint(bytes, &mut cursor)?;
        let field_number = tag / 8;
        let wire_type = (tag & 7) as u8;
        if field_number == 0 {
            return Err(TranscriptMirrorProtobufDecodeError::InvalidFieldNumber);
        }
        skip_field(bytes, &mut cursor, wire_type, field_number)?;
        if field_number != 5 {
            output.extend_from_slice(&bytes[field_start..cursor.offset]);
        }
    }
    Ok(output)
}

pub fn decode_transcript_mirror_conversation_state(
    bytes: &[u8],
) -> Result<TranscriptMirrorConversationState, TranscriptMirrorProtobufDecodeError> {
    let decoded = decode_conversation_state_recovery_fields(bytes)?;
    Ok(TranscriptMirrorConversationState {
        root_prompt_messages_json: decoded.root_prompt_messages_json,
        turns: decoded.turns,
        summary_archives: decoded.summary_archives,
    })
}

pub fn decode_summary_archive_message_ids(
    bytes: &[u8],
) -> Result<Vec<Vec<u8>>, TranscriptMirrorProtobufDecodeError> {
    let mut cursor = Cursor { offset: 0 };
    let mut message_ids = Vec::new();
    while cursor.offset < bytes.len() {
        let tag = read_varint(bytes, &mut cursor)?;
        let field_number = tag / 8;
        let wire_type = (tag & 7) as u8;
        if field_number == 0 {
            return Err(TranscriptMirrorProtobufDecodeError::InvalidFieldNumber);
        }
        if field_number == 1 && wire_type == 2 {
            message_ids.push(read_bytes(bytes, &mut cursor)?);
            continue;
        }
        skip_field(bytes, &mut cursor, wire_type, field_number)?;
    }
    Ok(message_ids)
}



#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UserMessageIdentityFields {
    pub text: String,
    pub message_id: String,
}

pub fn decode_user_message_identity_fields(
    bytes: &[u8],
) -> Result<UserMessageIdentityFields, TranscriptMirrorProtobufDecodeError> {
    let mut cursor = Cursor { offset: 0 };
    let mut decoded = UserMessageIdentityFields::default();
    while cursor.offset < bytes.len() {
        let tag = read_varint(bytes, &mut cursor)?;
        let field_number = tag / 8;
        let wire_type = (tag & 7) as u8;
        match (field_number, wire_type) {
            (1, 2) => decoded.text = read_string(bytes, &mut cursor)?,
            (2, 2) => decoded.message_id = read_string(bytes, &mut cursor)?,
            _ => skip_field(bytes, &mut cursor, wire_type, field_number)?,
        }
    }
    Ok(decoded)
}

pub fn decode_conversation_turn_structure_fields(
    bytes: &[u8],
) -> Result<Option<ConversationTurnStructureFields>, TranscriptMirrorProtobufDecodeError> {
    let mut cursor = Cursor { offset: 0 };
    while cursor.offset < bytes.len() {
        let tag = read_varint(bytes, &mut cursor)?;
        let field_number = tag / 8;
        let wire_type = (tag & 7) as u8;
        if wire_type != 2 || (field_number != 1 && field_number != 2) {
            skip_field(bytes, &mut cursor, wire_type, field_number)?;
            continue;
        }
        let nested = read_bytes(bytes, &mut cursor)?;
        if field_number == 1 {
            let mut nested_cursor = Cursor { offset: 0 };
            let mut user_message = Vec::new();
            let mut steps = Vec::new();
            let mut request_id = None;
            while nested_cursor.offset < nested.len() {
                let nested_tag = read_varint(&nested, &mut nested_cursor)?;
                let nested_field = nested_tag / 8;
                let nested_wire = (nested_tag & 7) as u8;
                match (nested_field, nested_wire) {
                    (1, 2) => user_message = read_bytes(&nested, &mut nested_cursor)?,
                    (2, 2) => steps.push(read_bytes(&nested, &mut nested_cursor)?),
                    (3, 2) => request_id = Some(read_string(&nested, &mut nested_cursor)?),
                    _ => skip_field(
                        &nested,
                        &mut nested_cursor,
                        nested_wire,
                        nested_field,
                    )?,
                }
            }
            return Ok(Some(ConversationTurnStructureFields::Agent {
                user_message,
                steps,
                request_id,
            }));
        }

        let mut nested_cursor = Cursor { offset: 0 };
        let mut shell_command = Vec::new();
        let mut shell_output = Vec::new();
        while nested_cursor.offset < nested.len() {
            let nested_tag = read_varint(&nested, &mut nested_cursor)?;
            let nested_field = nested_tag / 8;
            let nested_wire = (nested_tag & 7) as u8;
            match (nested_field, nested_wire) {
                (1, 2) => shell_command = read_bytes(&nested, &mut nested_cursor)?,
                (2, 2) => shell_output = read_bytes(&nested, &mut nested_cursor)?,
                _ => skip_field(
                    &nested,
                    &mut nested_cursor,
                    nested_wire,
                    nested_field,
                )?,
            }
        }
        return Ok(Some(ConversationTurnStructureFields::Shell {
            shell_command,
            shell_output,
        }));
    }
    Ok(None)
}

pub fn decode_subagent_persisted_state_fields(
    bytes: &[u8],
) -> Result<SubagentPersistedStateFields, TranscriptMirrorProtobufDecodeError> {
    let mut cursor = Cursor { offset: 0 };
    let mut state = SubagentPersistedStateFields::default();
    while cursor.offset < bytes.len() {
        let tag = read_varint(bytes, &mut cursor)?;
        let field_number = tag / 8;
        let wire_type = (tag & 7) as u8;
        match (field_number, wire_type) {
            (1, 2) => state.conversation_state = Some(read_bytes(bytes, &mut cursor)?),
            (2, 0) => state.created_timestamp_ms = read_varint(bytes, &mut cursor)?,
            (3, 0) => state.last_used_timestamp_ms = read_varint(bytes, &mut cursor)?,
            (4, 2) => state.subagent_type = Some(read_bytes(bytes, &mut cursor)?),
            _ => skip_field(bytes, &mut cursor, wire_type, field_number)?,
        }
    }
    Ok(state)
}


#[cfg(test)]
mod tests {
    use super::{
        conversation_compaction_epoch, decode_conversation_state_recovery_fields,
        remove_conversation_state_token_details,
    };

    #[test]
    fn conversation_compaction_epoch_counts_external_and_self_summaries() {
        // field 13 (summary_archives), twice; field 17 (self_summary_count) = 3.
        let state = [0x6a, 0x01, 0x01, 0x6a, 0x01, 0x02, 0x88, 0x01, 0x03];
        assert_eq!(conversation_compaction_epoch(&state).expect("compaction epoch"), 5);

    #[test]
    fn recovery_fields_decode_token_details_and_turn_timing() {
        // field 5 = ConversationTokenDetails { used=120, max=1000 }
        // field 14 = StepTiming { duration=25, timestamp=200 }
        let state = [
            0x2a, 0x05, 0x08, 0x78, 0x10, 0xe8, 0x07,
            0x72, 0x05, 0x08, 0x19, 0x10, 0xc8, 0x01,
        ];
        let decoded =
            decode_conversation_state_recovery_fields(&state).expect("decode state");
        let token = decoded.token_details.expect("token details");
        assert_eq!(token.used_tokens, 120);
        assert_eq!(token.max_tokens, 1_000);
        assert_eq!(decoded.turn_timings.len(), 1);
        assert_eq!(decoded.turn_timings[0].duration_ms, 25);
        assert_eq!(decoded.turn_timings[0].timestamp_ms, 200);
    }

    #[test]
    fn stale_token_details_can_be_removed_without_reencoding_other_fields() {
        let state = [
            0x6a, 0x01, 0xaa,
            0x2a, 0x05, 0x08, 0x78, 0x10, 0xe8, 0x07,
            0x42, 0x01, 0xbb,
        ];
        let stripped =
            remove_conversation_state_token_details(&state).expect("strip token details");
        assert_eq!(stripped, [0x6a, 0x01, 0xaa, 0x42, 0x01, 0xbb]);
        let decoded =
            decode_conversation_state_recovery_fields(&stripped).expect("decode stripped");
        assert!(decoded.token_details.is_none());
        assert_eq!(decoded.summary_archives, vec![vec![0xaa]]);
        assert_eq!(decoded.turns, vec![vec![0xbb]]);
    }
    }
}
