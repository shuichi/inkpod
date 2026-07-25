use super::*;

pub(crate) fn parse_cell_number(name: &str) -> Option<u32> {
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    let bytes = stem.as_bytes();
    let end = bytes.iter().rposition(u8::is_ascii_digit)? + 1;
    let start = bytes[..end]
        .iter()
        .rposition(|byte| !byte.is_ascii_digit())
        .map_or(0, |index| index + 1);
    stem[start..end].parse().ok()
}

pub(crate) fn natural_cmp(left: &str, right: &str) -> Ordering {
    let left_bytes = left.as_bytes();
    let right_bytes = right.as_bytes();
    let (mut left_index, mut right_index) = (0, 0);
    while left_index < left_bytes.len() && right_index < right_bytes.len() {
        if left_bytes[left_index].is_ascii_digit() && right_bytes[right_index].is_ascii_digit() {
            let left_end = digit_run_end(left_bytes, left_index);
            let right_end = digit_run_end(right_bytes, right_index);
            let left_digits = &left[left_index..left_end];
            let right_digits = &right[right_index..right_end];
            let left_trimmed = left_digits.trim_start_matches('0');
            let right_trimmed = right_digits.trim_start_matches('0');
            let left_value = if left_trimmed.is_empty() {
                "0"
            } else {
                left_trimmed
            };
            let right_value = if right_trimmed.is_empty() {
                "0"
            } else {
                right_trimmed
            };
            let order = left_value
                .len()
                .cmp(&right_value.len())
                .then_with(|| left_value.cmp(right_value))
                .then_with(|| left_digits.len().cmp(&right_digits.len()));
            if order != Ordering::Equal {
                return order;
            }
            left_index = left_end;
            right_index = right_end;
        } else {
            let order = left_bytes[left_index]
                .to_ascii_lowercase()
                .cmp(&right_bytes[right_index].to_ascii_lowercase());
            if order != Ordering::Equal {
                return order;
            }
            left_index += 1;
            right_index += 1;
        }
    }
    left_bytes.len().cmp(&right_bytes.len())
}

fn digit_run_end(bytes: &[u8], start: usize) -> usize {
    let mut end = start;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    end
}
