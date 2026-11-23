pub fn score(query: &str, candidate: &str) -> Option<i32> {
    let query = query.to_lowercase();
    let candidate = candidate.to_lowercase();
    if query.is_empty() {
        return Some(0);
    }
    let mut chars = candidate.char_indices();
    let mut last = None;
    let mut score = 0;
    for needle in query.chars() {
        let (offset, _) = chars.find(|(_, character)| *character == needle)?;
        score += if offset == 0
            || candidate[..offset]
                .chars()
                .last()
                .is_some_and(|character| matches!(character, '/' | '_' | '-' | ' '))
        {
            12
        } else {
            2
        };
        if last.is_some_and(|previous| previous + 1 == offset) {
            score += 5;
        }
        last = Some(offset);
    }
    Some(score - (candidate.len() as i32 / 8))
}

pub fn session_search_score(
    query: &str,
    project: &str,
    path: &str,
    branch: &str,
    agent: &str,
    title: Option<&str>,
) -> Option<i32> {
    [
        score(query, project).map(|rank| rank + 30),
        score(query, agent).map(|rank| rank + 30),
        title
            .and_then(|title| score(query, title))
            .map(|rank| rank + 30),
        score(query, path),
        score(query, branch),
    ]
    .into_iter()
    .flatten()
    .max()
}
