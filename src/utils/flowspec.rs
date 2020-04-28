fn parse_flowspec_action(action: &str) -> Result<u64, ParseError> {
    let words: Vec<_> = action.split_whitespace().collect();
    if words.is_empty() {
        return Err(ParseError::new(String::from("No FlowSpec Action found")));
    }
    match words[0].to_lowercase().as_str() {
        "redirect" => {
            if words.len() < 2 {
                return Err(ParseError::new(String::from(
                    "Redirect must provide a community",
                )));
            }
            let mut comm_bytes = [0u8; 8];
            comm_bytes[0..2].clone_from_slice(&[0x80, 0x08]); // Flowspec Redirect
            Community::try_from(words[1])
                .map(|comm| match comm {
                    Community::STANDARD(comm) => {
                        let bytes = transform_u32_to_bytes(comm);
                        comm_bytes[2..4].clone_from_slice(&[bytes[0], bytes[1]]);
                        comm_bytes[4..6].clone_from_slice(&[0; 2]);
                        comm_bytes[6..8].clone_from_slice(&[bytes[2], bytes[3]]);
                    }
                    _ => unreachable!(),
                })
                .map_err(|_| {
                    ParseError::new(format!("Unable to parse redirect community '{}'", words[1]))
                })?;
            Ok(u64::from_be_bytes(comm_bytes))
        }
        _ => Err(ParseError::new(format!(
            "Unsupported Flowspec Action: {}",
            words[0]
        ))),
    }
}

fn parse_flowspec_match(action: &str) -> Result<FlowspecFilter, ParseError> {
    let words: Vec<_> = action.split_whitespace().collect();
    if words.is_empty() {
        return Err(ParseError::new(String::from("No FlowSpec Match found")));
    }
    match words[0].to_lowercase().as_str() {
        "destination" => {
            if words.len() < 2 {
                return Err(ParseError::new(String::from(
                    "Prefix must provide a community",
                )));
            }
            let dest = prefix_from_str(&words[1])
                .map_err(|_| ParseError::new(format!("Unable to parse prefix '{}'", words[1])))?;
            Ok(FlowspecFilter::DestinationPrefix(dest))
        }
        "source" => {
            if words.len() < 2 {
                return Err(ParseError::new(String::from(
                    "Prefix must provide a community",
                )));
            }
            let src = prefix_from_str(&words[1])
                .map_err(|_| ParseError::new(format!("Unable to parse prefix '{}'", words[1])))?;
            Ok(FlowspecFilter::SourcePrefix(src))
        }
        _ => Err(ParseError::new(format!(
            "Unsupported Flowspec Action: {}",
            words[0]
        ))),
    }
}
