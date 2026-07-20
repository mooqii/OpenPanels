fn main() {
    let entry_skill = std::fs::read_to_string("../../skills/myopenpanels/SKILL.md")
        .expect("read MyOpenPanels Entry Skill");
    let metadata_value = |name: &str| {
        entry_skill
            .lines()
            .find_map(|line| line.trim().strip_prefix(&format!("{name}:")))
            .map(|value| value.trim().trim_matches(['\"', '\'']))
            .filter(|value| !value.is_empty())
    };
    let entry_skill_version =
        metadata_value("version").expect("MyOpenPanels Entry Skill metadata version");
    let entry_skill_source =
        metadata_value("source").expect("MyOpenPanels Entry Skill metadata source");
    println!("cargo:rustc-env=MYOPENPANELS_ENTRY_SKILL_VERSION={entry_skill_version}");
    println!("cargo:rustc-env=MYOPENPANELS_ENTRY_SKILL_SOURCE={entry_skill_source}");
    println!("cargo:rerun-if-changed=../../skills/myopenpanels/SKILL.md");
    println!("cargo:rerun-if-changed=../../apps/studio/src");
    println!("cargo:rerun-if-changed=../../apps/studio/dist");
    println!("cargo:rerun-if-changed=../../apps/studio/index.html");
    println!("cargo:rerun-if-changed=../../apps/studio/package.json");
    println!("cargo:rerun-if-changed=../../agent-resources");
}
