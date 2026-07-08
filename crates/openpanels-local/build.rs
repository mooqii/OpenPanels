fn main() {
    println!("cargo:rerun-if-changed=../../apps/local-studio/src");
    println!("cargo:rerun-if-changed=../../apps/local-studio/index.html");
    println!("cargo:rerun-if-changed=../../apps/local-studio/package.json");
    println!("cargo:rerun-if-changed=../../agent-guides");
}
