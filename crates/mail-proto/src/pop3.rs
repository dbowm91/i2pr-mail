pub const MAX_LINE: usize = 8192;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pop3Reply {
    Status { positive: bool, text: String },
    Multiline(Vec<Vec<u8>>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pop3Error {
    LineTooLong,
    InvalidStatus,
    MissingTerminator,
}

pub fn parse_status(line: &[u8]) -> Result<Pop3Reply, Pop3Error> {
    if line.len() > MAX_LINE {
        return Err(Pop3Error::LineTooLong);
    }
    let line = line.strip_suffix(b"\r\n").unwrap_or(line);
    if line.len() < 3 {
        return Err(Pop3Error::InvalidStatus);
    }
    let positive = match &line[..3] {
        b"+OK" => true,
        b"-ER" => false,
        _ => return Err(Pop3Error::InvalidStatus),
    };
    Ok(Pop3Reply::Status {
        positive,
        text: String::from_utf8_lossy(&line[3..]).trim().to_owned(),
    })
}

pub fn parse_multiline(lines: &[Vec<u8>]) -> Result<Pop3Reply, Pop3Error> {
    let mut out = Vec::new();
    for line in lines {
        if line.len() > MAX_LINE {
            return Err(Pop3Error::LineTooLong);
        }
        if line == b".\r\n" || line == b"." {
            return Ok(Pop3Reply::Multiline(out));
        }
        if line.starts_with(b"..") {
            out.push(line[1..].to_vec());
        } else {
            out.push(line.clone());
        }
    }
    Err(Pop3Error::MissingTerminator)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handles_status_and_dot_stuffing() {
        assert!(matches!(
            parse_status(b"+OK ready\r\n"),
            Ok(Pop3Reply::Status { positive: true, .. })
        ));
        assert_eq!(
            parse_multiline(&[b"..dot\r\n".to_vec(), b".\r\n".to_vec()]),
            Ok(Pop3Reply::Multiline(vec![b".dot\r\n".to_vec()]))
        );
    }
    #[test]
    fn rejects_unbounded_or_unterminated_input() {
        assert_eq!(
            parse_status(&vec![b'x'; MAX_LINE + 1]),
            Err(Pop3Error::LineTooLong)
        );
        assert_eq!(
            parse_multiline(&[b"data".to_vec()]),
            Err(Pop3Error::MissingTerminator)
        );
    }
}
