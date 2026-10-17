pub fn choose(value: bool) -> u8 {
    if value {
        1
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn cover_both_branches() {
        assert_ne!(std::env::var("CRAP_FIXTURE_FAIL_TEST").as_deref(), Ok("true"));
        assert_eq!(super::choose(true), 1);
        assert_eq!(super::choose(false), 2);
    }
}
