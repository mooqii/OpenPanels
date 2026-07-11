fn main() {
    println!("cargo:rerun-if-changed=../../apps/studio/src");
    println!("cargo:rerun-if-changed=../../apps/studio/dist");
    println!("cargo:rerun-if-changed=../../apps/studio/index.html");
    println!("cargo:rerun-if-changed=../../apps/studio/package.json");
    println!("cargo:rerun-if-changed=../../agent-resources");
}
