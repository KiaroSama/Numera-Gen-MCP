/// Accept only branch CI or the merge ref GitHub creates for a pull request.
pub fn valid_reference(event: &str, reference: &str) -> bool {
    if reference.chars().any(char::is_whitespace) {
        return false;
    }
    match event {
        "workflow_dispatch" | "push" => reference
            .strip_prefix("refs/heads/")
            .is_some_and(|name| !name.is_empty()),
        "pull_request" => reference
            .strip_prefix("refs/pull/")
            .and_then(|name| name.strip_suffix("/merge"))
            .is_some_and(|number| {
                number.bytes().all(|byte| byte.is_ascii_digit())
                    && number.parse::<u64>().is_ok_and(|number| number > 0)
            }),
        _ => false,
    }
}
