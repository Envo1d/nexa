/// Keep the current completion range intact when no next file exists: closing
/// commits that same range, whereas advancing consumes it into the position.
pub(super) fn advance_position(position: &mut usize, completed: usize, queue_len: usize) -> bool {
    let next = *position + completed;
    if next >= queue_len {
        return false;
    }

    *position = next;
    true
}

#[cfg(test)]
mod tests;
