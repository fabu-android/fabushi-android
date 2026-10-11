pub fn strip_context_fields(input: &str) -> String {
    let mut output = input.to_string();
    loop {
        let Some(start) = output.find("<context>") else { break; };
        let Some(relative_end) = output[start + 9..].find("</context>") else {
            output.replace_range(start.., "[context stripped]");
            break;
        };
        let end = start + 9 + relative_end + 10;
        output.replace_range(start..end, "[context stripped]");
    }
    output
}
