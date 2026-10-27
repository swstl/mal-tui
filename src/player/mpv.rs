// hands ani-cli's exact mpv arguments back to mal-tui
pub fn run() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    println!("__MAL_MPV__\t{}", args.join("\t"));
}
