/// Remove the common ANSI/OSC controls emitted by a TUI so assertions can
/// compare visible content without terminal cursor noise.
pub fn normalize_ansi(bytes: &[u8]) -> String {
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != 0x1b {
            output.push(bytes[index]);
            index += 1;
            continue;
        }
        index += 1;
        if index >= bytes.len() {
            break;
        }
        match bytes[index] {
            b'[' => {
                index += 1;
                while index < bytes.len() && !(0x40..=0x7e).contains(&bytes[index]) {
                    index += 1;
                }
                index = index.saturating_add(1);
            }
            b']' => {
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == 0x07 {
                        index += 1;
                        break;
                    }
                    if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b'\\') {
                        index += 2;
                        break;
                    }
                    index += 1;
                }
            }
            _ => index += 1,
        }
    }
    String::from_utf8_lossy(&output).into_owned()
}

pub fn visible_lines(bytes: &[u8]) -> Vec<String> {
    normalize_ansi(bytes)
        .lines()
        .map(|line| line.trim_end().to_owned())
        .filter(|line| !line.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_csi_and_osc_without_dropping_text() {
        let bytes = b"hello\x1b[2Jworld\x1b]52;c;Zm9v\x07!";
        assert_eq!(normalize_ansi(bytes), "helloworld!");
        assert_eq!(visible_lines(b"a  \n\x1b[31mb\x1b[0m\n"), vec!["a", "b"]);
        assert_eq!(normalize_ansi("中文\x1b[2K終".as_bytes()), "中文終");
    }
}
