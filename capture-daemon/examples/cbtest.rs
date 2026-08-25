use arboard::Clipboard;
fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    let mut c = Clipboard::new().unwrap();
    if mode == "set" {
        c.set_text(std::env::args().nth(2).unwrap()).unwrap();
        println!("SET OK");
    } else {
        match c.get_text() {
            Ok(t) => println!("GOT: {}", t),
            Err(e) => println!("ERR: {}", e),
        }
    }
}
