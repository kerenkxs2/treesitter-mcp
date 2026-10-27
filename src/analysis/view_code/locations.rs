use super::*;

pub(super) fn parse_definition_location(value: &Value) -> Result<DefinitionLocation, io::Error> {
    let file = value
        .get("file")
        .or_else(|| value.get("file_path"))
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .or_else(|| {
            value
                .get("uri")
                .and_then(Value::as_str)
                .and_then(file_path_from_uri)
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Definition location is missing 'file', 'file_path', or 'uri'",
            )
        })?;

    if let Some(start) = value
        .get("range")
        .and_then(|range| range.get("start"))
        .and_then(Value::as_object)
    {
        let line = start.get("line").and_then(Value::as_u64).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "LSP range is missing start.line",
            )
        })? as usize
            + 1;
        let column = start
            .get("character")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "LSP range is missing start.character",
                )
            })? as usize
            + 1;

        return Ok(DefinitionLocation { file, line, column });
    }

    let line = value.get("line").and_then(Value::as_u64).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Definition location is missing 1-based 'line'",
        )
    })? as usize;

    let column = value
        .get("col")
        .or_else(|| value.get("column"))
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Definition location is missing 1-based 'col' or 'column'",
            )
        })? as usize;

    Ok(DefinitionLocation { file, line, column })
}

pub(super) fn file_path_from_uri(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let path = rest.strip_prefix("localhost").unwrap_or(rest);
    Some(PathBuf::from(percent_decode(path)))
}

pub(super) fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[index + 1..index + 3]) {
                if let Ok(value) = u8::from_str_radix(hex, 16) {
                    output.push(value as char);
                    index += 3;
                    continue;
                }
            }
        }

        output.push(bytes[index] as char);
        index += 1;
    }

    output
}
