pub fn screen_contains(output: &[u8], needle: &str) -> bool {
    let visible = visible_screen_text(output);
    let compact_visible: String = visible.split_whitespace().collect();
    let compact_needle: String = needle.split_whitespace().collect();
    !compact_needle.is_empty() && compact_visible.contains(&compact_needle)
}

pub fn visible_screen_text(output: &[u8]) -> String {
    let mut visible = Vec::with_capacity(output.len());
    let mut index = 0;
    while index < output.len() {
        match output[index] {
            0x1b => skip_escape_sequence(output, &mut index),
            b'\n' | b'\t' => {
                visible.push(output[index]);
                index += 1;
            }
            0x20..=0x7e | 0x80..=0xff => {
                visible.push(output[index]);
                index += 1;
            }
            _ => index += 1,
        }
    }
    String::from_utf8_lossy(&visible).into_owned()
}

fn skip_escape_sequence(output: &[u8], index: &mut usize) {
    *index += 1;
    let Some(kind) = output.get(*index).copied() else {
        return;
    };
    *index += 1;
    match kind {
        b'[' => {
            while let Some(byte) = output.get(*index).copied() {
                *index += 1;
                if (0x40..=0x7e).contains(&byte) {
                    break;
                }
            }
        }
        b']' => {
            while let Some(byte) = output.get(*index).copied() {
                *index += 1;
                if byte == 0x07 {
                    break;
                }
                if byte == 0x1b && output.get(*index) == Some(&b'\\') {
                    *index += 1;
                    break;
                }
            }
        }
        _ => {}
    }
}
