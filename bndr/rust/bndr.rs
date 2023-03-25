fn escape(a: String) -> String {
    return a.replace("\n", "<NEWLINE>").replace("\r", "<CR_NEWLINE>")
}
pub fn send_str(a: &str) {
    let clon = &a.clone().to_string();
    println!("[SPWN_BINDING_OUT​]\"{}\"", escape(clon.to_string()));
}

pub fn send_int(a: i32) {
    println!("[SPWN_BINDING_OUT​]{}", a)
}

pub fn get_arguments() -> Vec<String> {
    let mut args: Vec<_> = ::std::env::args().collect();
    args.remove(0);
    args = args.iter().map(|x| x.replace("<SP13>", " ")).collect::<Vec<String>>();
    return args;
}

pub fn send_vec(a: Vec<String>) {
    println!("[SPWN_BINDING_OUT​]{:?}", a);
}
