pub fn run() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let referrer = args
        .iter()
        .find_map(|a| a.strip_prefix("--referrer="))
        .unwrap_or("");

    let url = args
        .iter()
        .rfind(|a| !a.starts_with("--"))
        .map(|s| s.as_str())
        .unwrap_or("");

    println!("__MAL_MPV__\t{}\t{}", url, referrer);
}
