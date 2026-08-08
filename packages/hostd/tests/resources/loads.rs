use super::*;

#[test]
fn loads_context_files_from_ancestors_general_to_specific() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("repo");
    let nested = project.join("a").join("b");
    fs::create_dir_all(&nested).unwrap();
    // Mark project/ as workspace root so find_workspace_root stops here.
    fs::create_dir_all(project.join(".git")).unwrap();
    fs::write(project.join("AGENTS.md"), "project").unwrap();
    fs::write(nested.join("AGENTS.md"), "nested").unwrap();

    let files = load_context_files(&nested);
    let contents = files
        .iter()
        .map(|file| file.content.as_str())
        .filter(|content| *content == "project" || *content == "nested")
        .collect::<Vec<_>>();

    assert_eq!(contents, vec!["project", "nested"]);
}

#[test]
fn loads_and_expands_prompt_templates() {
    let temp = tempfile::tempdir().unwrap();
    let prompts = temp.path().join(".piko").join("prompts");
    fs::create_dir_all(&prompts).unwrap();
    fs::write(
        prompts.join("fix.md"),
        "---\ndescription: Fix a bug\nargument-hint: <file>\n---\nFix $1 with $ARGUMENTS",
    )
    .unwrap();

    let templates = load_prompt_templates(temp.path());
    assert_eq!(templates[0].name, "fix");
    assert_eq!(
        expand_prompt_template("/fix src/main.rs now", &templates),
        "Fix src/main.rs with src/main.rs now"
    );
}

#[test]
fn skips_malformed_prompt_templates() {
    let temp = tempfile::tempdir().unwrap();
    let prompts = temp.path().join(".piko").join("prompts");
    fs::create_dir_all(&prompts).unwrap();
    fs::write(prompts.join("bad.md"), "---\n: invalid\n---\nBad").unwrap();
    fs::write(
        prompts.join("good.md"),
        "---\ndescription: Good\n---\nGood $1",
    )
    .unwrap();

    let templates = load_prompt_templates(temp.path());
    assert_eq!(templates.len(), 1);
    assert_eq!(templates[0].name, "good");
}

#[test]
fn expands_prompt_template_argument_slices_and_quotes() {
    let templates = vec![piko_hostd::domain::prompts::PromptTemplate {
        name: "slice".into(),
        description: "Slice args".into(),
        argument_hint: None,
        content: "one=$1 two=$2 ten=$10 all=$@ rest=${@:2} pair=${@:2:2} missing=$11 quoted=$3"
            .into(),
        file_path: std::path::PathBuf::from("slice.md"),
    }];

    assert_eq!(
        expand_prompt_template("/slice a1 a2 a3 a4 a5 a6 a7 a8 a9 a10", &templates),
        "one=a1 two=a2 ten=a10 all=a1 a2 a3 a4 a5 a6 a7 a8 a9 a10 rest=a2 a3 a4 a5 a6 a7 a8 a9 a10 pair=a2 a3 missing= quoted=a3"
    );

    assert_eq!(
        expand_prompt_template("/slice alpha beta 'gamma delta' epsilon", &templates),
        "one=alpha two=beta ten= all=alpha beta gamma delta epsilon rest=beta gamma delta epsilon pair=beta gamma delta missing= quoted=gamma delta"
    );
}

#[test]
fn snapshots_semantic_context_skills_and_templates() {
    let temp = tempfile::tempdir().unwrap();
    let skill_dir = temp.path().join(".piko").join("skills").join("demo");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: demo\ndescription: Demo skill\n---\nBody",
    )
    .unwrap();
    let skills = load_skills(temp.path()).skills;
    let snapshot = snapshot_prompt_resources(PromptSnapshotOptions {
        cwd: temp.path().to_path_buf(),
        context_files: vec![piko_hostd::domain::prompts::ContextFile {
            path: temp.path().join("AGENTS.md"),
            content: "project rules".into(),
        }],
        skills,
        prompt_templates: vec![piko_hostd::domain::prompts::PromptTemplate {
            name: "fix".into(),
            description: "Fix".into(),
            argument_hint: None,
            content: "Fix".into(),
            file_path: temp.path().join(".piko/prompts/fix.md"),
        }],
        ..PromptSnapshotOptions::default()
    });
    let prompt = snapshot
        .blocks
        .iter()
        .map(|block| block.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(prompt.contains("project rules"));
    assert!(prompt.contains("Available skill metadata"));
    assert!(prompt.contains("Prompt templates expanded by hostd"));
    assert!(prompt.contains("Current date: 20"));
    assert!(!prompt.contains("unix-day-"));
}
