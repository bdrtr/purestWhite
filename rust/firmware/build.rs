fn main() {
    // esp-hal's linker script, which pulls in the rest.
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}
