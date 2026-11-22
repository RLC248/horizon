//! The SDK's source-staging command for replacement methods below the game crate.
use horizon_mod_api_gen::functions::FunctionIndex;
use mod_surface::build_data::{self, Build};
use std::fs;
use std::path::Path;
use std::process::Command;

fn put(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn fixture(goals: &str) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    put(
        root,
        "crates/horizon-client/Cargo.toml",
        "[package]\nname = 'horizon-client'\n[dependencies]\nhorizon-engine-slots = { path = '../horizon-engine-slots' }\n",
    );
    put(
        root,
        "crates/horizon-client/src/lib.rs",
        "mod game; mod entry;",
    );
    put(root, "crates/horizon-client/src/game.rs", "");
    put(root, "crates/horizon-client/src/entry.rs", "");
    put(
        root,
        "crates/horizon-engine-slots/Cargo.toml",
        "[package]\nname = 'horizon-engine-slots'\n",
    );
    put(
        root,
        "Cargo.lock",
        "version = 4\n\n[[package]]\nname = 'horizon-client'\nversion = '0.1.0'\n",
    );
    put(root, "crates/horizon-mod-api-gen/internals.txt", "");
    put(
        root,
        "crates/lower/Cargo.toml",
        "[package]\nname = 'lower'\n",
    );
    put(root, "crates/lower/src/lib.rs", "mod goals;\n");
    put(root, "crates/lower/src/goals.rs", goals);
    temp
}

fn stage(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_horizon-stage"));
    command
        .arg(root)
        .arg(root.join("points.json"))
        .arg(root.join("internals.json"))
        .arg("c".repeat(40));
    command
}

fn succeeds(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn lower_trait_methods_stage_with_independent_slots() {
    let goals = r#"
struct Tempt {
    cooldown: u32,
}

trait Goal {
    fn can_use(&self) -> u32;
}

trait OtherGoal {
    fn can_use(&self) -> u32;
}

fn offset() -> u32 { 1 }

impl Goal for Tempt {
    #[horizon_mod_api_macros::replaceable]
    fn can_use(&self) -> u32 { self.cooldown + offset() }
}

impl crate::goals::OtherGoal for Tempt {
    #[horizon_mod_api_macros::replaceable]
    fn can_use(&self) -> u32 { self.cooldown * 2 }
}

impl Tempt {
    fn can_use(&self) -> u32 { 3 }
}
"#;
    let temp = fixture(goals);
    let root = temp.path();
    succeeds(&mut stage(root));

    let json = fs::read_to_string(root.join("functions.json")).unwrap();
    let (_, index): (Build, FunctionIndex) = build_data::read("functions.json", &json).unwrap();
    let target = &index.functions["lower::goals::<Tempt as Goal>::can_use"];
    let [start, end] = target.span;
    assert!(goals[start..end].contains("self.cooldown + offset()"));
    assert!(
        index
            .functions
            .contains_key("lower::goals::<Tempt as OtherGoal>::can_use")
    );
    assert!(!index.functions.contains_key("lower::goals::Tempt::can_use"));

    // The marker itself is a no-op procedural attribute. Omit it for this direct
    // rustc fixture, which compiles the staged source without a Cargo workspace.
    let file = root.join("crates/lower/src/goals.rs");
    let staged = fs::read_to_string(&file).unwrap();
    fs::write(
        &file,
        staged.replace("#[horizon_mod_api_macros::replaceable]", ""),
    )
    .unwrap();
    let library = root.join("liblower.rlib");
    succeeds(
        Command::new("rustc")
            .args([
                "--edition=2024",
                "--crate-name=lower",
                "--crate-type=rlib",
                "-Dwarnings",
            ])
            .arg(root.join("crates/lower/src/lib.rs"))
            .arg("-o")
            .arg(&library),
    );

    // A consumer uses the widened type and its fields and installs the replacements.
    put(
        root,
        "consumer.rs",
        r#"
use lower::goals::{Goal, OtherGoal, Tempt, offset};

fn main() {
    let tempt = Tempt { cooldown: 8 };
    assert_eq!(Goal::can_use(&tempt), 9);
    assert_eq!(OtherGoal::can_use(&tempt), 16);
    assert!(Tempt::__fnr_as_Goal_can_use(|goal| goal.cooldown + offset() + 100));
    assert_eq!(Goal::can_use(&tempt), 109);
    assert_eq!(OtherGoal::can_use(&tempt), 16);
    assert!(Tempt::__fnr_as_OtherGoal_can_use(|goal| goal.cooldown + 200));
    assert_eq!(OtherGoal::can_use(&tempt), 208);
    assert_eq!(Goal::can_use(&tempt), 109);
    assert_eq!(tempt.can_use(), 3);
}
"#,
    );
    let program = root.join(format!("consumer{}", std::env::consts::EXE_SUFFIX));
    succeeds(
        Command::new("rustc")
            .args(["--edition=2024", "-Dwarnings", "--extern"])
            .arg(format!("lower={}", library.display()))
            .arg(root.join("consumer.rs"))
            .arg("-o")
            .arg(&program),
    );
    succeeds(&mut Command::new(program));
}

#[test]
fn a_marker_reports_when_distinct_traits_share_its_target_key() {
    let goals = r#"
struct Tempt;
mod one {
    pub trait Goal { fn can_use(&self); }
}
mod two {
    pub trait Goal { fn can_use(&self); }
}

impl one::Goal for Tempt {
    #[horizon_mod_api_macros::replaceable]
    fn can_use(&self) {}
}

impl two::Goal for Tempt {
    fn can_use(&self) {}
}
"#;
    let temp = fixture(goals);
    let output = stage(temp.path()).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("on lower::goals::<Tempt as Goal>::can_use: several implementations share this replacement target"),
        "{stderr}"
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("crates/lower/src/goals.rs")).unwrap(),
        goals
    );
}
