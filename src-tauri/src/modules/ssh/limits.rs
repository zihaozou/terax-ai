pub const MAX_CHANNELS_PER_SPACE: usize = 32;
pub const MAX_AUTH_PROMPTS: usize = 16;
pub const MAX_PROMPT_BYTES: usize = 8 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 64 * 1024;
pub const MAX_INCLUDE_DEPTH: usize = 8;
pub const MAX_INCLUDE_FILES: usize = 64;
pub const MAX_CONFIG_FILE_BYTES: usize = 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_limits_match_the_spec() {
        assert_eq!(MAX_CHANNELS_PER_SPACE, 32);
        assert_eq!(MAX_AUTH_PROMPTS, 16);
        assert_eq!(MAX_PROMPT_BYTES, 8 * 1024);
        assert_eq!(MAX_RESPONSE_BYTES, 64 * 1024);
        assert_eq!(MAX_INCLUDE_DEPTH, 8);
        assert_eq!(MAX_INCLUDE_FILES, 64);
        assert_eq!(MAX_CONFIG_FILE_BYTES, 1024 * 1024);
    }
}
