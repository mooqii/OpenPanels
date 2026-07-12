fn main() {
    let entry_skill = std::fs::read_to_string("../../skills/myopenpanels/SKILL.md")
        .expect("read MyOpenPanels Entry Skill");
    let entry_skill_version = entry_skill
        .lines()
        .find_map(|line| line.trim().strip_prefix("version:"))
        .map(|value| value.trim().trim_matches(['\"', '\'']))
        .filter(|value| !value.is_empty())
        .expect("MyOpenPanels Entry Skill metadata version");
    println!("cargo:rustc-env=MYOPENPANELS_ENTRY_SKILL_VERSION={entry_skill_version}");
    println!("cargo:rerun-if-changed=../../skills/myopenpanels/SKILL.md");
    println!("cargo:rerun-if-changed=../../apps/studio/src");
    println!("cargo:rerun-if-changed=../../apps/studio/dist");
    println!("cargo:rerun-if-changed=../../apps/studio/index.html");
    println!("cargo:rerun-if-changed=../../apps/studio/package.json");
    println!("cargo:rerun-if-changed=../../agent-resources");
}
