fn main() {
    println!("cargo:rerun-if-env-changed=RUSTMAIL_GOOGLE_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=RUSTMAIL_GOOGLE_CLIENT_SECRET");
    tauri_build::build()
}
