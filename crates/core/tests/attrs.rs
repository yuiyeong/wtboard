mod common;

use common::TestRepo;
use wtboard_core::git::attrs::linguist_generated;
use wtboard_core::git::Git;

#[test]
fn reads_linguist_generated_like_github() {
    let repo = TestRepo::new();
    repo.write(
        ".gitattributes",
        "gen/** linguist-generated\ny.txt linguist-generated=true\nx.txt -linguist-generated\ngen/keep.ts -linguist-generated\n",
    );
    let paths = ["gen/a.ts", "gen/keep.ts", "x.txt", "y.txt", "z.txt", "공백 있는 파일.txt"];
    let mut got: Vec<String> = linguist_generated(&Git::new(repo.path()), &paths).unwrap().into_iter().collect();
    got.sort();
    assert_eq!(got, ["gen/a.ts", "y.txt"]);
}

#[test]
fn empty_path_list_needs_no_git_call() {
    let got = linguist_generated(&Git::new("/nonexistent"), &[]).unwrap();
    assert!(got.is_empty());
}
