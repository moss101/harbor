fn main() {
    let agent = ureq::AgentBuilder::new().redirects(0).build();
    for url in [
        "https://huggingface.co/api/models/Qwen/Qwen2.5-1.5B-Instruct-GGUF",
        "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf",
    ] {
        match agent.get(url).call() {
            Ok(r) => println!("OK {} -> {}", url.split('/').nth(3).unwrap_or(""), r.status()),
            Err(e) => println!("ERR {url} : {e}"),
        }
    }
}
