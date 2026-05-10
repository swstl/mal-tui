use std::io::{self, Read};

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut dp = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in dp.iter_mut().enumerate() { row[0] = i; }
    for (j, val) in dp[0].iter_mut().enumerate() { *val = j; }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            dp[i][j] = if a[i-1] == b[j-1] {
                dp[i-1][j-1]
            } else {
                1 + dp[i-1][j].min(dp[i][j-1]).min(dp[i-1][j-1])
            };
        }
    }
    dp[a.len()][b.len()]
}

fn extract_title(line: &str) -> &str {
    // lines are like: "1 Sword Art Online (25 episodes)"
    // strip leading "<number> " and trailing " (<n> episodes)"
    let without_index = line.trim().split_once(' ').map(|x| x.1).unwrap_or(line.trim());
    if let Some(pos) = without_index.rfind(" (") {
        &without_index[..pos]
    } else {
        without_index
    }
}

pub fn run() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();

    let target = std::env::var("ANICLI_TARGET").unwrap_or_default();
    let target_lower = target.to_lowercase();

    let best = input
        .lines()
        .filter(|l| !l.trim().is_empty())
        .min_by_key(|line| levenshtein(&extract_title(line).to_lowercase(), &target_lower));

    if let Some(line) = best {
        println!("{}", line);
    }
}
